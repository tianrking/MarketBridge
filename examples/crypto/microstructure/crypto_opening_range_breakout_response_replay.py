#!/usr/bin/env python3
"""Replay real-price responses around a fixed UTC opening-range breakout.

For each UTC day, the first N source candles define a fixed high/low opening
range.  Later candles are labelled first bullish/bearish breaks, persistent
outside states or inside-range controls.  The range is an explicit session
proxy, not a true exchange open, support level or execution model.
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


def opening_range_series(rows, opening_bars=4, breakout_buffer_bps=0.0):
    if opening_bars <= 0 or breakout_buffer_bps < 0:
        return []
    series = [None] * len(rows)
    current_day = None
    day_bars = 0
    opening_high = None
    opening_low = None
    for index, row in enumerate(rows):
        day = utc_day(row["ts_ms"])
        if day != current_day:
            current_day = day
            day_bars = 0
            opening_high = None
            opening_low = None
        day_bars += 1
        if day_bars <= opening_bars:
            opening_high = row["high"] if opening_high is None else max(opening_high, row["high"])
            opening_low = row["low"] if opening_low is None else min(opening_low, row["low"])
            continue
        if opening_high is None or opening_low is None:
            continue
        upper = opening_high * (1.0 + breakout_buffer_bps / 10_000.0)
        lower = opening_low * (1.0 - breakout_buffer_bps / 10_000.0)
        close = row["close"]
        above, below = close > upper, close < lower
        prior = series[index - 1]
        if above and (prior is None or prior.get("utc_day") != day
                      or prior["state"] not in {"bullish_breakout", "bullish_outside"}):
            state, direction, event = "bullish_breakout", 1, "bullish_orb_breakout"
        elif below and (prior is None or prior.get("utc_day") != day
                        or prior["state"] not in {"bearish_breakout", "bearish_outside"}):
            state, direction, event = "bearish_breakout", -1, "bearish_orb_breakout"
        elif above:
            state, direction, event = "bullish_outside", 1, None
        elif below:
            state, direction, event = "bearish_outside", -1, None
        else:
            state, direction, event = "inside_opening_range", 0, None
        series[index] = {
            "utc_day": day,
            "opening_high": opening_high,
            "opening_low": opening_low,
            "opening_range_width_bps": (opening_high / opening_low - 1.0) * 10_000.0,
            "breakout_buffer_bps": breakout_buffer_bps,
            "direction_sign": direction,
            "event_direction_sign": direction if event else None,
            "state": state,
            "event": event,
        }
    return series


def opening_range_features(rows, index, opening_bars=4, breakout_buffer_bps=0.0):
    series = opening_range_series(rows, opening_bars, breakout_buffer_bps)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, opening_bars, breakout_buffer_bps, horizon_bars):
    if opening_bars <= 0 or breakout_buffer_bps < 0 or horizon_bars <= 0:
        raise ValueError("invalid opening range, buffer or horizon")
    series = opening_range_series(rows, opening_bars, breakout_buffer_bps)
    observations = []
    for index in range(len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        path = rows[index + 1:index + horizon_bars + 1]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = features["event_direction_sign"] or features["direction_sign"]
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
    states = ("bullish_breakout", "bearish_breakout", "bullish_outside",
              "bearish_outside", "inside_opening_range")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_orb_breakout", "bearish_orb_breakout")
    }
    events_count = sum(bucket["observations"] for bucket in events.values())
    control = by_state["inside_opening_range"]["observations"]
    sufficient = events_count >= min_observations and control >= min_observations
    return {
        "observations": len(observations),
        "opening_breakout_observations": events_count,
        "inside_range_control_observations": control,
        "by_state": by_state,
        "by_event": events,
        "verdict": ("opening_range_response_reported" if sufficient
                     else "observe_only_insufficient_breakout_or_control"),
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
    parser.add_argument("--opening-bars", type=int, default=4)
    parser.add_argument("--breakout-buffer-bps", type=float, default=0.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.opening_bars <= 0
            or args.breakout_buffer_bps < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid opening range, buffer, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.opening_bars, args.breakout_buffer_bps,
                                      args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_opening_range_breakout_response_replay",
        "hypothesis": "fixed UTC opening-range breakouts may have different later responses from same-day inside-range controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"session": "UTC_day_start", "opening_bars": args.opening_bars,
                    "breakout_buffer_bps": args.breakout_buffer_bps,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "UTC day start is a crypto research session convention, not a universal exchange open",
            "the first returned candle of a UTC day is treated as bar one; incomplete history is not backfilled",
            "opening bar count, buffer and row-count horizon are caller-supplied sensitivity parameters",
            "breakout labels do not prove continuation, support/resistance, fills, stops or execution",
            "missing bars, fees, funding, slippage, queue and fills are not inferred",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
