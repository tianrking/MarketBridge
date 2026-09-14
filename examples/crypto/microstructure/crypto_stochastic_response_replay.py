#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Stochastic oscillator states.

The implementation computes raw %K from the current close's position in an
inclusive high/low window, then applies simple moving-average smoothing for
%K and %D.  It labels overbought/oversold/neutral states and K/D crosses, then
measures a fixed future response.  It is a momentum study, not a reversal,
entry or execution model.
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


def _sma(values, period):
    if len(values) < period:
        return None
    return statistics.mean(values[-period:])


def stochastic_series(rows, k_period=14, smooth_k=3, smooth_d=3,
                      overbought=80.0, oversold=20.0):
    """Return smoothed %K/%D features without using future bars."""
    if (k_period <= 0 or smooth_k <= 0 or smooth_d <= 0
            or not 0 <= oversold < overbought <= 100):
        return []
    series = [None] * len(rows)
    raw_values, smoothed_values = [], []
    for index in range(len(rows)):
        if index < k_period - 1:
            raw_values.append(None)
            smoothed_values.append(None)
            continue
        window = rows[index - k_period + 1:index + 1]
        high = max(row["high"] for row in window)
        low = min(row["low"] for row in window)
        if high <= low:
            raw_values.append(None)
            smoothed_values.append(None)
            continue
        raw = 100.0 * (rows[index]["close"] - low) / (high - low)
        raw_values.append(raw)
        raw_window = raw_values[index - smooth_k + 1:index + 1]
        smoothed = (_sma(raw_window, smooth_k)
                    if len(raw_window) == smooth_k and all(value is not None for value in raw_window)
                    else None)
        smoothed_values.append(smoothed)
        if smoothed is None:
            continue
        d_window = smoothed_values[index - smooth_d + 1:index + 1]
        d_value = (_sma(d_window, smooth_d)
                   if len(d_window) == smooth_d and all(value is not None for value in d_window)
                   else None)
        if d_value is None:
            continue
        if smoothed >= overbought and d_value >= overbought:
            state, direction = "overbought", -1
        elif smoothed <= oversold and d_value <= oversold:
            state, direction = "oversold", 1
        else:
            state, direction = "neutral", 0
        prior = series[index - 1]
        event = None
        if prior is not None:
            if prior["k"] <= prior["d"] and smoothed > d_value:
                event = "bullish_kd_cross"
            elif prior["k"] >= prior["d"] and smoothed < d_value:
                event = "bearish_kd_cross"
        event_direction = (1 if event == "bullish_kd_cross"
                           else -1 if event == "bearish_kd_cross" else None)
        series[index] = {
            "raw_k": raw,
            "k": smoothed,
            "d": d_value,
            "high": high,
            "low": low,
            "overbought": overbought,
            "oversold": oversold,
            "direction_sign": direction,
            "event_direction_sign": event_direction,
            "state": state,
            "event": event,
        }
    return series


def stochastic_features(rows, index, k_period=14, smooth_k=3, smooth_d=3,
                         overbought=80.0, oversold=20.0):
    series = stochastic_series(rows, k_period, smooth_k, smooth_d, overbought, oversold)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, k_period, smooth_k, smooth_d, overbought, oversold, horizon_bars):
    if (k_period <= 0 or smooth_k <= 0 or smooth_d <= 0 or horizon_bars <= 0
            or not 0 <= oversold < overbought <= 100):
        raise ValueError("invalid stochastic periods, thresholds or horizon")
    series = stochastic_series(rows, k_period, smooth_k, smooth_d, overbought, oversold)
    observations = []
    for index in range(len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        path = rows[index + 1:index + horizon_bars + 1]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = (features["event_direction_sign"]
                     if features["event_direction_sign"] is not None
                     else features["direction_sign"])
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": features["state"],
            "event": features["event"],
            "features": features,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0 if direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(row["low"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(row["high"] for row in path)
                                             / rows[index]["close"] - 1.0) * 100.0,
        })
    return observations


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    aligned = [row["direction_aligned_return_bps"] for row in rows
               if row["direction_aligned_return_bps"] is not None]
    absolute = [row["forward_absolute_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_direction_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "mean_absolute_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(observations, min_observations):
    states = ("overbought", "oversold", "neutral")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_kd_cross", "bearish_kd_cross")
    }
    extremes = by_state["overbought"]["observations"] + by_state["oversold"]["observations"]
    control = by_state["neutral"]["observations"]
    sufficient = extremes >= min_observations and control >= min_observations
    return {
        "observations": len(observations),
        "extreme_observations": extremes,
        "neutral_control_observations": control,
        "kd_cross_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("stochastic_response_reported" if sufficient
                     else "observe_only_insufficient_extreme_or_neutral_control"),
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
    parser.add_argument("--k-period", type=int, default=14)
    parser.add_argument("--smooth-k", type=int, default=3)
    parser.add_argument("--smooth-d", type=int, default=3)
    parser.add_argument("--overbought", type=float, default=80.0)
    parser.add_argument("--oversold", type=float, default=20.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.k_period <= 0
            or args.smooth_k <= 0 or args.smooth_d <= 0
            or not 0 <= args.oversold < args.overbought <= 100
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid stochastic periods, thresholds, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.k_period, args.smooth_k, args.smooth_d,
                                      args.overbought, args.oversold, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_stochastic_response_replay",
        "hypothesis": "point-in-time stochastic extremes and K/D crosses may have different later responses from neutral controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"k_period": args.k_period, "smooth_k": args.smooth_k, "smooth_d": args.smooth_d,
                    "overbought": args.overbought, "oversold": args.oversold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "%K uses an inclusive OHLC high/low range and simple moving-average %K/%D smoothing",
            "zero-range windows are excluded rather than assigning an artificial oscillator value",
            "thresholds, periods and row-count horizon are caller-supplied sensitivity parameters",
            "extremes and K/D crosses are descriptive labels, not reversal, entry, stop or execution rules",
            "overlapping windows omit fees, funding, slippage, queue and fills and do not establish causality",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
