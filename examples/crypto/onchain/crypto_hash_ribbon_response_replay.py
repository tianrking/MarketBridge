#!/usr/bin/env python3
"""Replay BTC response after provider-estimated hash-ribbon states.

The case translates the public 30-day/60-day hashrate-crossing idea into a
descriptive, timestamp-aware test.  MarketBridge supplies mempool.space
hashrate history and BTC candles; the replay measures later fixed-day returns
after a recovery cross, a capitulation cross, or ordinary ribbon states.  It
does not identify miners, estimate profitability, or turn a cross into an
investment signal.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timezone
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def utc_date(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000.0, timezone.utc).date().isoformat()


def hashrate_points(payload):
    points = []
    for row in payload.get("hashrates", []):
        ts_ms = row.get("ts_ms")
        hashrate = number(row.get("avg_hashrate_hs"))
        if isinstance(ts_ms, int) and hashrate is not None and hashrate > 0:
            points.append((ts_ms, hashrate))
    return sorted({ts_ms: value for ts_ms, value in points}.items())


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            points.append((utc_date(ts_ms), close, ts_ms))
    return sorted({date: (close, ts_ms) for date, close, ts_ms in points}.items())


def rolling_mean(points, as_of_ms, window_days):
    if window_days <= 0:
        raise ValueError("window_days must be positive")
    start_ms = as_of_ms - window_days * 86_400_000
    rows = [(ts_ms, value) for ts_ms, value in points if start_ms <= ts_ms <= as_of_ms]
    if not rows or rows[0][0] > start_ms:
        return None
    return statistics.mean(value for _, value in rows)


def price_sma(prices, date, window_days):
    dates = [item[0] for item in prices]
    try:
        index = dates.index(date)
    except ValueError:
        return None
    start = index - window_days + 1
    if start < 0:
        return None
    return statistics.mean(item[1][0] for item in prices[start:index + 1])


def classify_cross(previous_short, previous_long, short_ma, long_ma, price_momentum_positive):
    if None in (previous_short, previous_long, short_ma, long_ma):
        return "observe_only_insufficient_hashrate_window"
    if previous_short <= previous_long and short_ma > long_ma:
        return ("hashrate_recovery_with_positive_price_momentum"
                if price_momentum_positive else "hashrate_recovery_without_positive_price_momentum")
    if previous_short >= previous_long and short_ma < long_ma:
        return "hashrate_capitulation_cross"
    if short_ma > long_ma:
        return "hashrate_expansion"
    if short_ma < long_ma:
        return "hashrate_contraction"
    return "hashrate_balanced"


def aligned_observations(
    hashrates,
    prices,
    short_window_days,
    long_window_days,
    price_short_days,
    price_long_days,
    horizon_days,
):
    if min(short_window_days, long_window_days, price_short_days, price_long_days, horizon_days) <= 0:
        raise ValueError("all windows must be positive")
    if short_window_days >= long_window_days:
        raise ValueError("short hashrate window must be smaller than long window")
    if price_short_days >= price_long_days:
        raise ValueError("short price window must be smaller than long window")
    price_dates = [date for date, _ in prices]
    price_index = {date: index for index, date in enumerate(price_dates)}
    rows = []
    previous = None
    for ts_ms, _ in hashrates:
        date = utc_date(ts_ms)
        if date not in price_index:
            continue
        short_ma = rolling_mean(hashrates, ts_ms, short_window_days)
        long_ma = rolling_mean(hashrates, ts_ms, long_window_days)
        previous_short, previous_long = previous or (None, None)
        current_price_sma = price_sma(prices, date, price_short_days)
        long_price_sma = price_sma(prices, date, price_long_days)
        momentum_positive = (current_price_sma is not None and long_price_sma is not None
                             and current_price_sma > long_price_sma)
        if short_ma is None or long_ma is None:
            previous = (short_ma, long_ma)
            continue
        future_index = price_index[date] + horizon_days
        if future_index >= len(price_dates):
            previous = (short_ma, long_ma)
            continue
        future_date = price_dates[future_index]
        current_price = prices[price_index[date]][1][0]
        future_price = prices[future_index][1][0]
        forward = (future_price / current_price - 1.0) * 100.0
        rows.append({
            "date": date,
            "ts_ms": ts_ms,
            "hashrate_short_ma_hs": short_ma,
            "hashrate_long_ma_hs": long_ma,
            "hashrate_ratio": short_ma / long_ma if long_ma else None,
            "price_short_sma": current_price_sma,
            "price_long_sma": long_price_sma,
            "state": classify_cross(previous_short, previous_long, short_ma, long_ma,
                                     momentum_positive),
            "forward_date": future_date,
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
        })
        previous = (short_ma, long_ma)
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize(observations, min_observations):
    states = (
        "hashrate_recovery_with_positive_price_momentum",
        "hashrate_recovery_without_positive_price_momentum",
        "hashrate_capitulation_cross", "hashrate_expansion",
        "hashrate_contraction", "hashrate_balanced",
        "observe_only_insufficient_hashrate_window",
    )
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    recovery = (by_state["hashrate_recovery_with_positive_price_momentum"]["observations"]
                + by_state["hashrate_recovery_without_positive_price_momentum"]["observations"])
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "recovery_crosses": recovery,
        "verdict": ("hash_ribbon_response_reported" if recovery >= min_observations
                     else "observe_only_insufficient_hash_ribbon_crosses"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--mining-window", default="3y")
    parser.add_argument("--mining-limit", type=int, default=5000)
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=1_095.0)
    parser.add_argument("--candle-limit", type=int, default=1500)
    parser.add_argument("--short-window-days", type=int, default=30)
    parser.add_argument("--long-window-days", type=int, default=60)
    parser.add_argument("--price-short-days", type=int, default=10)
    parser.add_argument("--price-long-days", type=int, default=20)
    parser.add_argument("--horizon-days", type=int, default=30)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.mining_limit <= 5000
            or not 2 <= args.candle_limit <= 1500 or args.min_observations <= 0
            or args.timeout <= 0):
        parser.error("invalid days, limits, observation count or timeout")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    mining_payload = fetch(args.base_url, "/v1/history/mining", {
        "window": args.mining_window, "limit": args.mining_limit,
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.candle_limit,
    }, args.timeout)
    hashrates = hashrate_points(mining_payload)
    prices = candle_points(candle_payload)
    observations = aligned_observations(
        hashrates, prices, args.short_window_days, args.long_window_days,
        args.price_short_days, args.price_long_days, args.horizon_days,
    )
    print(json.dumps({
        "strategy": "crypto_hash_ribbon_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "mining_scope": {"provider": "mempool_space", "window": args.mining_window},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "short_window_days": args.short_window_days,
            "long_window_days": args.long_window_days,
            "price_short_days": args.price_short_days,
            "price_long_days": args.price_long_days,
            "horizon_days": args.horizon_days,
            "min_observations": args.min_observations,
        },
        "source_counts": {"hashrate_rows": len(hashrates), "price_bars": len(prices),
                           "aligned_observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": {"mining": mining_payload.get("coverage_detail"),
                     "price": candle_payload.get("coverage_detail")},
        "upstream_errors": ([mining_payload["error"]] if mining_payload.get("error") else [])
        + ([candle_payload["error"]] if candle_payload.get("error") else []),
        "limitations": [
            "hashrate is a provider estimate with provider-controlled cadence and revisions",
            "30/60-day windows require timestamp coverage and are not a miner revenue or profitability model",
            "price momentum is a close-only filter, not a fill, leverage, fee or allocation model",
            "crosses are descriptive associations; sample size, look-ahead, causality and execution remain outside scope",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
