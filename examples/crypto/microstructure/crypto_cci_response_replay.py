#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Commodity Channel Index states.

CCI measures typical-price deviation from its trailing SMA scaled by mean
deviation.  This transparent implementation labels positive/negative extremes,
neutral values and zero-line crosses, then measures a fixed future response.
It is a deviation study, not a reversal or execution model.
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


def cci_series(rows, period=20, constant=0.015, overbought=100.0, oversold=-100.0):
    if period <= 0 or constant <= 0 or oversold >= overbought:
        return []
    series = [None] * len(rows)
    typical = [(row["high"] + row["low"] + row["close"]) / 3.0 for row in rows]
    for index in range(period - 1, len(rows)):
        window = typical[index - period + 1:index + 1]
        mean = statistics.mean(window)
        deviation = statistics.mean(abs(value - mean) for value in window)
        if deviation <= 0:
            continue
        cci = (typical[index] - mean) / (constant * deviation)
        if cci >= overbought:
            state, direction = "overbought", -1
        elif cci <= oversold:
            state, direction = "oversold", 1
        elif cci > 0:
            state, direction = "positive_neutral", 1
        elif cci < 0:
            state, direction = "negative_neutral", -1
        else:
            state, direction = "neutral", 0
        prior = series[index - 1]
        event = None
        if prior is not None:
            if prior["cci"] <= 0 < cci:
                event = "positive_zero_cross"
            elif prior["cci"] >= 0 > cci:
                event = "negative_zero_cross"
        event_direction = (1 if event == "positive_zero_cross"
                           else -1 if event == "negative_zero_cross" else None)
        series[index] = {
            "typical_price": typical[index],
            "sma_typical_price": mean,
            "mean_deviation": deviation,
            "cci": cci,
            "constant": constant,
            "overbought": overbought,
            "oversold": oversold,
            "direction_sign": direction,
            "event_direction_sign": event_direction,
            "state": state,
            "event": event,
        }
    return series


def cci_features(rows, index, period=20, constant=0.015, overbought=100.0, oversold=-100.0):
    series = cci_series(rows, period, constant, overbought, oversold)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, period, constant, overbought, oversold, horizon_bars):
    if period <= 0 or constant <= 0 or oversold >= overbought or horizon_bars <= 0:
        raise ValueError("invalid CCI period, thresholds, constant or horizon")
    series = cci_series(rows, period, constant, overbought, oversold)
    observations = []
    for index in range(period - 1, len(rows) - horizon_bars):
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
    states = ("overbought", "oversold", "positive_neutral", "negative_neutral", "neutral")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("positive_zero_cross", "negative_zero_cross")
    }
    extremes = by_state["overbought"]["observations"] + by_state["oversold"]["observations"]
    controls = sum(by_state[state]["observations"] for state in
                   ("positive_neutral", "negative_neutral", "neutral"))
    sufficient = extremes >= min_observations and controls >= min_observations
    return {
        "observations": len(observations),
        "extreme_observations": extremes,
        "neutral_control_observations": controls,
        "cci_cross_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("cci_response_reported" if sufficient
                     else "observe_only_insufficient_extreme_or_control"),
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
    parser.add_argument("--period", type=int, default=20)
    parser.add_argument("--constant", type=float, default=0.015)
    parser.add_argument("--overbought", type=float, default=100.0)
    parser.add_argument("--oversold", type=float, default=-100.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.period <= 0
            or args.constant <= 0 or args.oversold >= args.overbought
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid CCI period, thresholds, constant, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.period, args.constant, args.overbought,
                                      args.oversold, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_cci_response_replay",
        "hypothesis": "point-in-time CCI deviations beyond instrument thresholds may have different later responses from neutral deviations",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"period": args.period, "constant": args.constant,
                    "overbought": args.overbought, "oversold": args.oversold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "CCI uses typical-price SMA and mean deviation with an explicit 0.015 scaling constant",
            "thresholds, period, constant and row-count horizon are caller-supplied sensitivity parameters",
            "zero-deviation windows remain missing; extremes can indicate strength as well as reversal context",
            "CCI labels and zero crosses are descriptive, not reversal, entry, stop or execution rules",
            "overlapping windows omit fees, funding, slippage, queue and fills and do not establish causality",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
