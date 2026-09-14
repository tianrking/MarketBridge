#!/usr/bin/env python3
"""Replay BTC response after Mayer Multiple valuation-regime observations.

The Mayer Multiple is price divided by a 200-day simple moving average.  This
case tests whether fixed forward BTC responses differ across transparent
discount, trend-band and premium regimes.  It is a historical response table,
not a valuation claim, allocation rule, or trading instruction.
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


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            points.append((utc_date(ts_ms), close, ts_ms))
    return sorted({date: (close, ts_ms) for date, close, ts_ms in points}.items())


def classify_multiple(multiple, discount_threshold, deep_discount_threshold,
                      premium_threshold, extreme_premium_threshold):
    if multiple is None:
        return "observe_only_insufficient_ma_window"
    if multiple <= deep_discount_threshold:
        return "deep_discount"
    if multiple <= discount_threshold:
        return "discount"
    if multiple >= extreme_premium_threshold:
        return "extreme_premium"
    if multiple >= premium_threshold:
        return "premium"
    return "trend_band"


def aligned_observations(
    prices,
    ma_days,
    discount_threshold,
    deep_discount_threshold,
    premium_threshold,
    extreme_premium_threshold,
    horizon_days,
):
    if ma_days <= 0 or horizon_days <= 0:
        raise ValueError("ma_days and horizon_days must be positive")
    if not deep_discount_threshold < discount_threshold < premium_threshold < extreme_premium_threshold:
        raise ValueError("thresholds must be strictly ordered")
    rows = []
    for index in range(ma_days - 1, len(prices) - horizon_days):
        date, (close, ts_ms) = prices[index]
        window = [item[1][0] for item in prices[index - ma_days + 1:index + 1]]
        sma = statistics.mean(window)
        multiple = close / sma if sma > 0 else None
        future_date, (future_close, _) = prices[index + horizon_days]
        forward = (future_close / close - 1.0) * 100.0
        rows.append({
            "date": date,
            "ts_ms": ts_ms,
            "close": close,
            "sma_days": ma_days,
            "sma_close": sma,
            "mayer_multiple": multiple,
            "state": classify_multiple(
                multiple, discount_threshold, deep_discount_threshold,
                premium_threshold, extreme_premium_threshold,
            ),
            "forward_date": future_date,
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    multiples = [row["mayer_multiple"] for row in rows]
    return {
        "observations": len(rows),
        "mean_mayer_multiple": statistics.mean(multiples) if multiples else None,
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize(observations, min_observations):
    states = ("deep_discount", "discount", "trend_band", "premium", "extreme_premium")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    extreme = by_state["deep_discount"]["observations"] + by_state["extreme_premium"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "extreme_regime_windows": extreme,
        "verdict": ("mayer_multiple_response_reported" if extreme >= min_observations
                     else "observe_only_insufficient_mayer_extreme_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--days", type=float, default=1825.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--ma-days", type=int, default=200)
    parser.add_argument("--discount-threshold", type=float, default=0.8)
    parser.add_argument("--deep-discount-threshold", type=float, default=0.6)
    parser.add_argument("--premium-threshold", type=float, default=2.4)
    parser.add_argument("--extreme-premium-threshold", type=float, default=3.0)
    parser.add_argument("--horizon-days", type=int, default=30)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.days <= 0 or not 2 <= args.limit <= 1500 or args.min_observations <= 0 or args.timeout <= 0:
        parser.error("invalid days, limit, observation count or timeout")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    prices = candle_points(payload)
    observations = aligned_observations(
        prices, args.ma_days, args.discount_threshold, args.deep_discount_threshold,
        args.premium_threshold, args.extreme_premium_threshold, args.horizon_days,
    )
    print(json.dumps({
        "strategy": "crypto_mayer_multiple_response_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "ma_days": args.ma_days,
            "discount_threshold": args.discount_threshold,
            "deep_discount_threshold": args.deep_discount_threshold,
            "premium_threshold": args.premium_threshold,
            "extreme_premium_threshold": args.extreme_premium_threshold,
            "horizon_days": args.horizon_days,
            "min_observations": args.min_observations,
        },
        "source_counts": {"price_bars": len(prices), "aligned_observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Mayer Multiple is price divided by a close-only moving average, not intrinsic value or realized-cap cost basis",
            "thresholds are caller-supplied historical-regime labels, not universal valuation boundaries",
            "fixed close-to-close responses omit fees, funding, slippage, allocation, causality and execution",
            "candle retention, missing dates and small extreme-regime samples remain visible",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
