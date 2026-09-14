#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Supertrend states.

This is a transparent ATR-band proxy: a trailing simple ATR is combined with
the current candle's midpoint, then recursively clamped bands produce a
Supertrend direction.  A flip is labelled on the bar where the close crosses
the prior direction's active band.  The result is a descriptive response
study, not a forecast, stop rule, or execution model.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
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
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0
                and values["high"] >= values["open"] and values["high"] >= values["close"]
                and values["low"] <= values["open"] and values["low"] <= values["close"]):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def true_range(row, previous_close=None):
    if previous_close is None:
        return row["high"] - row["low"]
    return max(row["high"] - row["low"], abs(row["high"] - previous_close),
               abs(row["low"] - previous_close))


def supertrend_series(rows, atr_period, multiplier):
    """Return one point-in-time feature dict per candle, or None during warmup."""
    if atr_period <= 0 or multiplier <= 0:
        return []
    series = [None] * len(rows)
    true_ranges = []
    previous_direction = None
    previous_upper = None
    previous_lower = None
    for index, row in enumerate(rows):
        previous_close = rows[index - 1]["close"] if index else None
        tr = true_range(row, previous_close)
        true_ranges.append(tr)
        if index + 1 < atr_period:
            continue
        atr = statistics.mean(true_ranges[index + 1 - atr_period:index + 1])
        midpoint = (row["high"] + row["low"]) / 2.0
        basic_upper = midpoint + multiplier * atr
        basic_lower = midpoint - multiplier * atr
        if previous_upper is None:
            final_upper, final_lower = basic_upper, basic_lower
            direction = 1 if row["close"] >= midpoint else -1
        else:
            prior_close = rows[index - 1]["close"]
            final_upper = (basic_upper if basic_upper < previous_upper or prior_close > previous_upper
                           else previous_upper)
            final_lower = (basic_lower if basic_lower > previous_lower or prior_close < previous_lower
                           else previous_lower)
            direction = previous_direction
            if previous_direction <= 0 and row["close"] > final_upper:
                direction = 1
            elif previous_direction >= 0 and row["close"] < final_lower:
                direction = -1
        previous_direction, previous_upper, previous_lower = direction, final_upper, final_lower
        prior = series[index - 1]
        if prior is None:
            state = "bullish_trend" if direction > 0 else "bearish_trend"
        elif direction > 0 and prior["direction_sign"] <= 0:
            state = "bullish_flip"
        elif direction < 0 and prior["direction_sign"] >= 0:
            state = "bearish_flip"
        else:
            state = "bullish_trend" if direction > 0 else "bearish_trend"
        active_line = final_lower if direction > 0 else final_upper
        series[index] = {
            "atr": atr,
            "midpoint": midpoint,
            "basic_upper": basic_upper,
            "basic_lower": basic_lower,
            "final_upper": final_upper,
            "final_lower": final_lower,
            "supertrend": active_line,
            "direction_sign": direction,
            "state": state,
        }
    return series


def supertrend_features(rows, index, atr_period, multiplier):
    series = supertrend_series(rows, atr_period, multiplier)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, atr_period, multiplier, horizon_bars):
    if atr_period <= 0 or multiplier <= 0 or horizon_bars <= 0:
        raise ValueError("ATR period, multiplier and horizon must be positive")
    series = supertrend_series(rows, atr_period, multiplier)
    observations = []
    for index in range(atr_period - 1, len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = features["direction_sign"]
        path = rows[index + 1:index + horizon_bars + 1]
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": features["state"],
            "features": features,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(row["low"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(row["high"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
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
    states = ("bullish_flip", "bearish_flip", "bullish_trend", "bearish_trend")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    flips = by_state["bullish_flip"]["observations"] + by_state["bearish_flip"]["observations"]
    controls = by_state["bullish_trend"]["observations"] + by_state["bearish_trend"]["observations"]
    sufficient = flips >= min_observations and controls >= min_observations
    return {
        "observations": len(observations),
        "flip_observations": flips,
        "trend_control_observations": controls,
        "by_state": by_state,
        "verdict": ("supertrend_response_reported" if sufficient
                     else "observe_only_insufficient_flip_or_trend_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=180.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--atr-period", type=int, default=10)
    parser.add_argument("--multiplier", type=float, default=3.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.atr_period <= 0
            or args.multiplier <= 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid ATR, multiplier, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.atr_period, args.multiplier, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_supertrend_response_replay",
        "hypothesis": "point-in-time ATR-band trend flips may have different later aligned responses from persistent trend states",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"atr_period": args.atr_period, "multiplier": args.multiplier,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "ATR uses a transparent trailing simple mean of true ranges; platform implementations may use Wilder smoothing",
            "recursive band clamping is an explicit proxy, not a guarantee of TradingView or exchange indicator parity",
            "flip and trend labels are descriptive states, not entry, exit, stop or forecast rules",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
