#!/usr/bin/env python3
"""Replay BTC response after historical stablecoin-supply changes.

MarketBridge supplies both the DefiLlama stablecoin history and the aligned
BTC candle history.  The replay tests a narrow association: after a trailing
stablecoin-supply expansion or contraction, is the next fixed candle window
different from flat supply windows?  It does not treat supply as exchange
inventory, a capital-flow proof, or a trading signal.
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


def stablecoin_points(payload):
    points = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        value = number(row.get("total_circulating_usd"))
        if isinstance(ts_ms, int) and value is not None and value > 0:
            points.append((utc_date(ts_ms), value, ts_ms))
    return sorted({date: (value, ts_ms) for date, value, ts_ms in points}.items())


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            points.append((utc_date(ts_ms), close, ts_ms))
    return sorted({date: (close, ts_ms) for date, close, ts_ms in points}.items())


def classify_change(change_pct, threshold_pct):
    if change_pct is None:
        return "observe_only_missing_supply_change"
    if change_pct >= threshold_pct:
        return "stablecoin_supply_expansion"
    if change_pct <= -threshold_pct:
        return "stablecoin_supply_contraction"
    return "stablecoin_supply_flat"


def aligned_observations(stablecoins, prices, change_window_bars, threshold_pct, horizon_bars):
    if change_window_bars <= 0 or horizon_bars <= 0:
        raise ValueError("change_window_bars and horizon_bars must be positive")
    price_by_date = {date: value[0] for date, value in prices}
    price_dates = [date for date, _ in prices]
    price_index = {date: index for index, date in enumerate(price_dates)}
    rows = []
    for index, (date, (value, ts_ms)) in enumerate(stablecoins):
        previous_index = index - change_window_bars
        if previous_index < 0 or date not in price_index:
            continue
        future_index = price_index[date] + horizon_bars
        if future_index >= len(price_dates):
            continue
        previous = stablecoins[previous_index][1][0]
        if previous <= 0:
            continue
        change_pct = (value / previous - 1.0) * 100.0
        current_price = price_by_date[date]
        future_date = price_dates[future_index]
        future_price = price_by_date[future_date]
        forward_return_pct = (future_price / current_price - 1.0) * 100.0
        rows.append({
            "date": date,
            "ts_ms": ts_ms,
            "total_circulating_usd": value,
            "change_window_bars": change_window_bars,
            "change_pct": change_pct,
            "state": classify_change(change_pct, threshold_pct),
            "forward_date": future_date,
            "forward_return_pct": forward_return_pct,
            "forward_abs_return_pct": abs(forward_return_pct),
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    changes = [row["change_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_change_pct": statistics.mean(changes) if changes else None,
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(observations, min_observations):
    states = ("stablecoin_supply_expansion", "stablecoin_supply_contraction",
              "stablecoin_supply_flat", "observe_only_missing_supply_change")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    qualifying = (by_state["stablecoin_supply_expansion"]["observations"]
                  + by_state["stablecoin_supply_contraction"]["observations"])
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "qualifying_supply_change_windows": qualifying,
        "verdict": ("stablecoin_historical_response_reported"
                    if qualifying >= min_observations
                    else "observe_only_insufficient_historical_supply_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--chain", default="all")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=1825.0)
    parser.add_argument("--limit", type=int, default=5000)
    parser.add_argument("--change-window-bars", type=int, default=7)
    parser.add_argument("--threshold-pct", type=float, default=1.0)
    parser.add_argument("--horizon-bars", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 5000 or args.change_window_bars <= 0
            or args.threshold_pct < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid days, limit, windows, threshold or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    stablecoin_payload = fetch(args.base_url, "/v1/history/stablecoins", {
        "chain": args.chain, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.limit, 1500),
    }, args.timeout)
    stablecoins = stablecoin_points(stablecoin_payload)
    prices = candle_points(candle_payload)
    observations = aligned_observations(
        stablecoins, prices, args.change_window_bars, args.threshold_pct, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_stablecoin_historical_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "stablecoin_scope": {"chain": args.chain, "peg": "peggedUSD"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"change_window_bars": args.change_window_bars,
                    "threshold_pct": args.threshold_pct, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"stablecoin_rows": len(stablecoins), "price_bars": len(prices),
                           "aligned_forward_windows": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": {"stablecoin": stablecoin_payload.get("coverage_detail"),
                     "price": candle_payload.get("coverage_detail")},
        "upstream_errors": ([stablecoin_payload["error"]] if stablecoin_payload.get("error") else [])
        + ([candle_payload["error"]] if candle_payload.get("error") else []),
        "limitations": [
            "stablecoin history is a provider circulating-market-cap series, not exchange inventory or bridge flow",
            "DefiLlama historical values can be revised and cadence may differ from exchange daily candles",
            "UTC-date alignment, fixed close-to-close response, causality, fees and execution are not modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
