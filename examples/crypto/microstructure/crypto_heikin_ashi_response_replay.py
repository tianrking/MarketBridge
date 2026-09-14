#!/usr/bin/env python3
"""Replay real-price responses around point-in-time Heikin-Ashi states.

Heikin-Ashi OHLC is synthetic: the transformed candle is used only for
direction and wick-state labels, while forward returns always use the source
market candle close.  This is a trend-smoothing study, not an executable
synthetic-price strategy.
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


def heikin_ashi_series(rows, wick_tolerance_bps=1.0, doji_body_bps=5.0):
    if wick_tolerance_bps < 0 or doji_body_bps < 0:
        return []
    series = [None] * len(rows)
    for index, row in enumerate(rows):
        ha_close = (row["open"] + row["high"] + row["low"] + row["close"]) / 4.0
        if index == 0:
            ha_open = (row["open"] + row["close"]) / 2.0
        else:
            previous = series[index - 1]
            ha_open = (previous["ha_open"] + previous["ha_close"]) / 2.0
        ha_high = max(row["high"], ha_open, ha_close)
        ha_low = min(row["low"], ha_open, ha_close)
        body_bps = abs(ha_close - ha_open) / ha_close * 10_000.0 if ha_close else None
        lower_wick_bps = max(0.0, min(ha_open, ha_close) - ha_low) / ha_close * 10_000.0
        upper_wick_bps = max(0.0, ha_high - max(ha_open, ha_close)) / ha_close * 10_000.0
        if body_bps is not None and body_bps <= doji_body_bps:
            state, direction = "doji", 0
        elif ha_close > ha_open and lower_wick_bps <= wick_tolerance_bps:
            state, direction = "bullish_no_lower_wick", 1
        elif ha_close < ha_open and upper_wick_bps <= wick_tolerance_bps:
            state, direction = "bearish_no_upper_wick", -1
        elif ha_close > ha_open:
            state, direction = "bullish_mixed", 1
        else:
            state, direction = "bearish_mixed", -1
        prior = series[index - 1]
        event = None
        if prior is not None:
            if direction > 0 and prior["direction_sign"] < 0:
                event = "bullish_ha_flip"
            elif direction < 0 and prior["direction_sign"] > 0:
                event = "bearish_ha_flip"
        series[index] = {
            "ha_open": ha_open,
            "ha_high": ha_high,
            "ha_low": ha_low,
            "ha_close": ha_close,
            "body_bps": body_bps,
            "lower_wick_bps": lower_wick_bps,
            "upper_wick_bps": upper_wick_bps,
            "wick_tolerance_bps": wick_tolerance_bps,
            "doji_body_bps": doji_body_bps,
            "direction_sign": direction,
            "state": state,
            "event": event,
        }
    return series


def heikin_ashi_features(rows, index, wick_tolerance_bps=1.0, doji_body_bps=5.0):
    series = heikin_ashi_series(rows, wick_tolerance_bps, doji_body_bps)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, wick_tolerance_bps, doji_body_bps, horizon_bars):
    if wick_tolerance_bps < 0 or doji_body_bps < 0 or horizon_bars <= 0:
        raise ValueError("invalid Heikin-Ashi tolerances or horizon")
    series = heikin_ashi_series(rows, wick_tolerance_bps, doji_body_bps)
    observations = []
    for index in range(len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        path = rows[index + 1:index + horizon_bars + 1]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = features["direction_sign"]
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
    states = ("bullish_no_lower_wick", "bearish_no_upper_wick", "bullish_mixed",
              "bearish_mixed", "doji")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_ha_flip", "bearish_ha_flip")
    }
    strong = by_state["bullish_no_lower_wick"]["observations"] + by_state["bearish_no_upper_wick"]["observations"]
    controls = by_state["bullish_mixed"]["observations"] + by_state["bearish_mixed"]["observations"] + by_state["doji"]["observations"]
    sufficient = strong >= min_observations and controls >= min_observations
    return {
        "observations": len(observations),
        "strong_wickless_observations": strong,
        "mixed_or_doji_controls": controls,
        "ha_flip_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("heikin_ashi_response_reported" if sufficient
                     else "observe_only_insufficient_wickless_or_control"),
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
    parser.add_argument("--wick-tolerance-bps", type=float, default=1.0)
    parser.add_argument("--doji-body-bps", type=float, default=5.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.wick_tolerance_bps < 0
            or args.doji_body_bps < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid Heikin-Ashi tolerance, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.wick_tolerance_bps, args.doji_body_bps,
                                      args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_heikin_ashi_response_replay",
        "hypothesis": "point-in-time Heikin-Ashi wickless trend states may have different later real-price responses from mixed/doji controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"wick_tolerance_bps": args.wick_tolerance_bps, "doji_body_bps": args.doji_body_bps,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Heikin-Ashi OHLC is synthetic; all forward returns use the source candle close, not transformed prices",
            "wick tolerance, doji threshold and row-count horizon are caller-supplied sensitivity parameters",
            "synthetic trend/wick labels are descriptive, not execution prices, entries, stops or forecasts",
            "missing bars, fees, funding, slippage, queue and fills are not inferred",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
