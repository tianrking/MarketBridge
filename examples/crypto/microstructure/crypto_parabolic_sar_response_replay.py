#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Parabolic SAR states.

This is an explicit Wilder-style SAR recursion with an extreme point and an
acceleration factor.  It labels bullish/bearish flips and persistent trend
states, then measures a fixed future response.  The indicator's stop-and-
reverse name is kept as provenance only; this example never submits a stop or
order and is not an execution model.
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


def psar_series(rows, start_af=0.02, step=0.02, max_af=0.20):
    """Return one explicit SAR feature dict per bar after the two-bar warmup."""
    if start_af <= 0 or step <= 0 or max_af < start_af:
        return []
    series = [None] * len(rows)
    if len(rows) < 2:
        return series
    up = rows[1]["close"] >= rows[0]["close"]
    sar = rows[0]["low"] if up else rows[0]["high"]
    ep = rows[1]["high"] if up else rows[1]["low"]
    af = start_af
    series[1] = {
        "sar": sar,
        "extreme_point": ep,
        "acceleration_factor": af,
        "direction_sign": 1 if up else -1,
        "state": "bullish_trend" if up else "bearish_trend",
        "event": None,
        "sar_distance_bps": (rows[1]["close"] / sar - 1.0) * 10_000.0 if sar > 0 else None,
    }
    for index in range(2, len(rows)):
        prior_up = up
        candidate = sar + af * (ep - sar)
        if up:
            candidate = min(candidate, rows[index - 1]["low"], rows[index - 2]["low"])
            if rows[index]["low"] < candidate:
                up = False
                sar = ep
                ep = rows[index]["low"]
                af = start_af
            else:
                sar = candidate
                if rows[index]["high"] > ep:
                    ep = rows[index]["high"]
                    af = min(max_af, af + step)
        else:
            candidate = max(candidate, rows[index - 1]["high"], rows[index - 2]["high"])
            if rows[index]["high"] > candidate:
                up = True
                sar = ep
                ep = rows[index]["high"]
                af = start_af
            else:
                sar = candidate
                if rows[index]["low"] < ep:
                    ep = rows[index]["low"]
                    af = min(max_af, af + step)
        direction = 1 if up else -1
        event = ("bullish_flip" if up and not prior_up
                 else "bearish_flip" if not up and prior_up else None)
        series[index] = {
            "sar": sar,
            "extreme_point": ep,
            "acceleration_factor": af,
            "direction_sign": direction,
            "state": "bullish_trend" if up else "bearish_trend",
            "event": event,
            "sar_distance_bps": (rows[index]["close"] / sar - 1.0) * 10_000.0 if sar > 0 else None,
        }
    return series


def psar_features(rows, index, start_af=0.02, step=0.02, max_af=0.20):
    series = psar_series(rows, start_af, step, max_af)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, start_af, step, max_af, horizon_bars):
    if start_af <= 0 or step <= 0 or max_af < start_af or horizon_bars <= 0:
        raise ValueError("invalid SAR parameters or horizon")
    series = psar_series(rows, start_af, step, max_af)
    observations = []
    for index in range(1, len(rows) - horizon_bars):
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
    states = ("bullish_trend", "bearish_trend")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_flip", "bearish_flip")
    }
    flips = sum(bucket["observations"] for bucket in events.values())
    controls = sum(bucket["observations"] for bucket in by_state.values()) - flips
    sufficient = flips >= min_observations and controls >= min_observations
    return {
        "observations": len(observations),
        "flip_observations": flips,
        "trend_control_observations": controls,
        "by_state": by_state,
        "by_event": events,
        "verdict": ("parabolic_sar_response_reported" if sufficient
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
    parser.add_argument("--start-af", type=float, default=0.02)
    parser.add_argument("--step", type=float, default=0.02)
    parser.add_argument("--max-af", type=float, default=0.20)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.start_af <= 0
            or args.step <= 0 or args.max_af < args.start_af or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid SAR, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.start_af, args.step, args.max_af,
                                      args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_parabolic_sar_response_replay",
        "hypothesis": "point-in-time Parabolic SAR flips may have different later aligned responses from persistent trend states",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"start_af": args.start_af, "step": args.step, "max_af": args.max_af,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "SAR initialization, acceleration factors and two-prior-low/high clamp are explicit conventions",
            "a flip is a descriptive as-of state, not a stop, reversal guarantee, entry, forecast or order",
            "parameters and row-count horizon are caller-supplied sensitivity choices",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
