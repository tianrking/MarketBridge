#!/usr/bin/env python3
"""Replay OHLCV responses around prior-UTC-day traditional pivot levels.

Previous UTC-day high/low/close produce P, R1/S1 and R2/S2.  Current-day bars
are labelled by point-in-time relation to those levels, with R1 rejection and
S1 reclaim events kept separate from inside-range controls.  The levels are a
transparent OHLC proxy, not validated support/resistance or an execution model.
"""

import argparse
import json
import statistics
import time
from datetime import datetime, timezone
from urllib.parse import urlencode
from urllib.request import Request, urlopen


DAY_MS = 86_400_000


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


def utc_day(ts_ms):
    return datetime.fromtimestamp(ts_ms / 1000.0, tz=timezone.utc).date().isoformat()


def traditional_levels(previous):
    high, low, close = previous["high"], previous["low"], previous["close"]
    pivot = (high + low + close) / 3.0
    span = high - low
    return {
        "pivot": pivot,
        "r1": 2.0 * pivot - low,
        "s1": 2.0 * pivot - high,
        "r2": pivot + span,
        "s2": pivot - span,
    }


def pivot_series(rows, tolerance_bps=10.0):
    if tolerance_bps < 0:
        return []
    series = [None] * len(rows)
    previous_day = None
    previous_aggregate = None
    levels = None
    prior_features = None
    for index, row in enumerate(rows):
        day = utc_day(row["ts_ms"])
        if previous_day is None:
            previous_day = day
            previous_aggregate = {"high": row["high"], "low": row["low"], "close": row["close"]}
            continue
        if day != previous_day:
            levels = traditional_levels(previous_aggregate)
            previous_day = day
            previous_aggregate = {"high": row["high"], "low": row["low"], "close": row["close"]}
            prior_features = None
        else:
            previous_aggregate["high"] = max(previous_aggregate["high"], row["high"])
            previous_aggregate["low"] = min(previous_aggregate["low"], row["low"])
            previous_aggregate["close"] = row["close"]
        if levels is None:
            continue
        close = row["close"]
        band = levels["pivot"] * tolerance_bps / 10_000.0
        event = None
        event_direction = None
        if row["high"] >= levels["r1"] and close < levels["r1"]:
            event, event_direction = "r1_rejection", -1
        elif row["low"] <= levels["s1"] and close > levels["s1"]:
            event, event_direction = "s1_reclaim", 1
        if close > levels["r1"]:
            state, direction = "above_r1", 1
        elif close < levels["s1"]:
            state, direction = "below_s1", -1
        elif abs(close - levels["pivot"]) <= band:
            state, direction = "near_pivot", 0
        else:
            state, direction = "inside_pivot_range", 0
        series[index] = {
            "utc_day": day,
            "levels": levels.copy(),
            "distance_to_pivot_bps": (close / levels["pivot"] - 1.0) * 10_000.0,
            "distance_to_r1_bps": (close / levels["r1"] - 1.0) * 10_000.0,
            "distance_to_s1_bps": (close / levels["s1"] - 1.0) * 10_000.0,
            "tolerance_bps": tolerance_bps,
            "direction_sign": direction,
            "event_direction_sign": event_direction,
            "state": state,
            "event": event,
        }
        prior_features = series[index]
    return series


def pivot_features(rows, index, tolerance_bps=10.0):
    series = pivot_series(rows, tolerance_bps)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, tolerance_bps, horizon_bars):
    if tolerance_bps < 0 or horizon_bars <= 0:
        raise ValueError("invalid pivot tolerance or horizon")
    series = pivot_series(rows, tolerance_bps)
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
    states = ("r1_rejection", "s1_reclaim", "above_r1", "below_s1",
              "inside_pivot_range", "near_pivot")
    by_state = {state: bucket_stats([row for row in observations if row["event"] == state
                                     or row["state"] == state]) for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("r1_rejection", "s1_reclaim")
    }
    controls = by_state["inside_pivot_range"]["observations"] + by_state["near_pivot"]["observations"]
    events_count = sum(bucket["observations"] for bucket in events.values())
    sufficient = events_count >= min_observations and controls >= min_observations
    return {
        "observations": len(observations),
        "pivot_event_observations": events_count,
        "pivot_control_observations": controls,
        "by_state": by_state,
        "by_event": events,
        "verdict": ("pivot_response_reported" if sufficient
                     else "observe_only_insufficient_pivot_event_or_control"),
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
    parser.add_argument("--tolerance-bps", type=float, default=10.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.tolerance_bps < 0
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid pivot tolerance, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.tolerance_bps, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_pivot_response_replay",
        "hypothesis": "prior-UTC-day traditional pivot touches and reclaims may have different later responses from inside-range controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"pivot_type": "traditional", "pivot_timeframe": "UTC_day",
                    "tolerance_bps": args.tolerance_bps, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "traditional pivots use prior UTC-day OHLC and are an OHLC level proxy, not validated support/resistance",
            "UTC day boundaries, tolerance and row-count horizon are caller-supplied research conventions",
            "touch/reclaim labels are descriptive, not entries, exits, stops, forecasts or orders",
            "missing calendar days, fees, funding, slippage, queue and fills are not inferred",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
