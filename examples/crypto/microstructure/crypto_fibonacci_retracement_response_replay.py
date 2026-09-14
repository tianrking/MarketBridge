#!/usr/bin/env python3
"""Replay responses near point-in-time Fibonacci retracement levels.

For each candle, the swing high/low are selected only from a trailing window
that ends before the current candle.  The current close is mapped to a
direction-aware retracement ratio and grouped near 38.2%, 50% and 61.8% (or a
control range).  This is an OHLCV level-response study, not proof that a
Fibonacci level is support/resistance or a trade/execution rule.
"""

import argparse
import json
import statistics
import time
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


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        values = {key: number(row.get(key)) for key in ("high", "low", "close")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["close"] > 0 and values["high"] >= values["low"] > 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def swing_levels(rows, index, lookback_bars):
    if lookback_bars <= 0 or index < lookback_bars:
        return None
    window = rows[index - lookback_bars:index]
    high_index = max(range(len(window)), key=lambda offset: window[offset]["high"])
    low_index = min(range(len(window)), key=lambda offset: window[offset]["low"])
    swing_high, swing_low = window[high_index]["high"], window[low_index]["low"]
    if swing_high <= swing_low or high_index == low_index:
        return None
    direction = 1 if low_index < high_index else -1
    return {"high": swing_high, "low": swing_low,
            "direction": direction, "high_ts_ms": window[high_index]["ts_ms"],
            "low_ts_ms": window[low_index]["ts_ms"]}


def retracement_ratio(close, levels):
    if levels is None or close is None:
        return None
    span = levels["high"] - levels["low"]
    if span <= 0:
        return None
    if levels["direction"] > 0:
        return (levels["high"] - close) / span
    return (close - levels["low"]) / span


def classify_ratio(ratio, level_tolerance):
    if ratio is None:
        return "missing_swing"
    if ratio < 0.0 or ratio > 1.0:
        return "outside_swing_range"
    candidates = ((0.382, "fib_382"), (0.500, "fib_500"), (0.618, "fib_618"))
    nearest = min(candidates, key=lambda item: abs(ratio - item[0]))
    if abs(ratio - nearest[0]) <= level_tolerance:
        return nearest[1]
    return "inside_swing_range_other"


def build_observations(rows, lookback_bars, level_tolerance, horizon_bars):
    if lookback_bars <= 0 or horizon_bars <= 0 or level_tolerance < 0:
        raise ValueError("invalid lookback, tolerance or horizon")
    observations = []
    for index in range(lookback_bars, len(rows) - horizon_bars):
        levels = swing_levels(rows, index, lookback_bars)
        ratio = retracement_ratio(rows[index]["close"], levels)
        state = classify_ratio(ratio, level_tolerance)
        future = rows[index + horizon_bars]
        path = [rows[offset]["close"] for offset in range(index + 1, index + horizon_bars + 1)]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = levels["direction"] if levels else 0
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "close": rows[index]["close"],
            "swing": levels,
            "retracement_ratio": ratio,
            "state": state,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(path) / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / rows[index]["close"] - 1.0) * 100.0,
        })
    return observations


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    aligned = [row["direction_aligned_return_bps"] for row in rows]
    absolute = [row["forward_absolute_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_direction_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "mean_absolute_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(observations, min_observations):
    states = ("fib_382", "fib_500", "fib_618", "inside_swing_range_other",
              "outside_swing_range", "missing_swing")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    return {
        "aligned_forward_windows": len(observations),
        "by_state": by_state,
        "verdict": ("fibonacci_response_reported" if len(observations) >= min_observations
                     else "observe_only_insufficient_fibonacci_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="4h")
    parser.add_argument("--days", type=float, default=730.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=90)
    parser.add_argument("--level-tolerance", type=float, default=0.03)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.lookback_bars <= 0
            or args.level_tolerance < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid swing, tolerance, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.lookback_bars, args.level_tolerance, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_fibonacci_retracement_response_replay",
        "hypothesis": "point-in-time 38.2, 50 and 61.8 percent retracement zones may have different later BTC responses",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars,
                    "level_tolerance": args.level_tolerance,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "swing high and low are a trailing-window proxy, not discretionary chart anchors",
            "windows whose extrema occur on the same candle remain missing_swing instead of receiving an arbitrary direction",
            "retracement ratios, tolerance, lookback and horizon are caller-supplied sensitivity parameters",
            "levels are not guaranteed support/resistance and no volume or trend confirmation is inferred",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, allocation and fills",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
