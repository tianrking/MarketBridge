#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time ADX/DMI states.

The implementation follows an explicit Wilder-style smoothing convention for
true range, directional movement, DI and ADX.  It labels strong/weak direction
and +DI/-DI crosses, then measures a fixed future response.  ADX is a trend
strength context, not a directional forecast or an execution model.
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


def directional_inputs(rows):
    true_ranges, plus_moves, minus_moves = [], [], []
    for index, row in enumerate(rows):
        if index == 0:
            true_ranges.append(row["high"] - row["low"])
            plus_moves.append(0.0)
            minus_moves.append(0.0)
            continue
        previous = rows[index - 1]
        up_move = row["high"] - previous["high"]
        down_move = previous["low"] - row["low"]
        true_ranges.append(max(row["high"] - row["low"],
                               abs(row["high"] - previous["close"]),
                               abs(row["low"] - previous["close"])))
        plus_moves.append(up_move if up_move > down_move and up_move > 0 else 0.0)
        minus_moves.append(down_move if down_move > up_move and down_move > 0 else 0.0)
    return true_ranges, plus_moves, minus_moves


def dmi_series(rows, period, strength_threshold=25.0):
    """Return ADX/DMI features once the first Wilder ADX is available."""
    if period <= 0 or strength_threshold < 0:
        return []
    series = [None] * len(rows)
    if len(rows) < 2 * period - 1:
        return series
    true_ranges, plus_moves, minus_moves = directional_inputs(rows)
    smooth_tr = smooth_plus = smooth_minus = 0.0
    dx_values = []
    previous_adx = None
    previous_direction = None
    for index in range(len(rows)):
        if index == period - 1:
            smooth_tr = sum(true_ranges[:period])
            smooth_plus = sum(plus_moves[:period])
            smooth_minus = sum(minus_moves[:period])
        elif index >= period:
            smooth_tr = smooth_tr - smooth_tr / period + true_ranges[index]
            smooth_plus = smooth_plus - smooth_plus / period + plus_moves[index]
            smooth_minus = smooth_minus - smooth_minus / period + minus_moves[index]
        else:
            continue
        if smooth_tr <= 0:
            continue
        plus_di = 100.0 * smooth_plus / smooth_tr
        minus_di = 100.0 * smooth_minus / smooth_tr
        di_sum = plus_di + minus_di
        dx = 100.0 * abs(plus_di - minus_di) / di_sum if di_sum > 0 else 0.0
        dx_values.append(dx)
        if len(dx_values) < period:
            continue
        if previous_adx is None:
            adx = statistics.mean(dx_values[-period:])
        else:
            adx = (previous_adx * (period - 1) + dx) / period
        direction = 1 if plus_di > minus_di else -1 if minus_di > plus_di else 0
        if adx >= strength_threshold:
            if direction > 0:
                state = "strong_bullish"
            elif direction < 0:
                state = "strong_bearish"
            else:
                state = "strong_mixed"
        elif direction > 0:
            state = "weak_bullish"
        elif direction < 0:
            state = "weak_bearish"
        else:
            state = "range_or_mixed"
        di_cross = (previous_direction is not None and direction != 0
                    and previous_direction != 0 and direction != previous_direction)
        event = ("bullish_di_cross" if di_cross and direction > 0
                 else "bearish_di_cross" if di_cross else None)
        series[index] = {
            "plus_di": plus_di,
            "minus_di": minus_di,
            "di_spread": plus_di - minus_di,
            "adx": adx,
            "adx_change": adx - previous_adx if previous_adx is not None else None,
            "direction_sign": direction,
            "strength_threshold": strength_threshold,
            "state": state,
            "di_cross": di_cross,
            "event": event,
        }
        previous_adx = adx
        previous_direction = direction if direction != 0 else previous_direction
    return series


def dmi_features(rows, index, period, strength_threshold=25.0):
    series = dmi_series(rows, period, strength_threshold)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, period, strength_threshold, horizon_bars):
    if period <= 0 or strength_threshold < 0 or horizon_bars <= 0:
        raise ValueError("period, strength threshold and horizon must be valid")
    series = dmi_series(rows, period, strength_threshold)
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
    states = ("strong_bullish", "strong_bearish", "strong_mixed", "weak_bullish",
              "weak_bearish", "range_or_mixed")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_di_cross", "bearish_di_cross")
    }
    strong = sum(by_state[state]["observations"] for state in
                 ("strong_bullish", "strong_bearish", "strong_mixed"))
    control_states = ("weak_bullish", "weak_bearish", "range_or_mixed")
    control = sum(by_state[state]["observations"] for state in control_states)
    sufficient = strong >= min_observations and control >= min_observations
    return {
        "observations": len(observations),
        "strong_trend_observations": strong,
        "weak_or_range_control_observations": control,
        "di_cross_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("adx_dmi_response_reported" if sufficient
                     else "observe_only_insufficient_strong_or_weak_control"),
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
    parser.add_argument("--period", type=int, default=14)
    parser.add_argument("--strength-threshold", type=float, default=25.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.period <= 0
            or args.strength_threshold < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid period, strength, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.period, args.strength_threshold, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_adx_dmi_response_replay",
        "hypothesis": "point-in-time ADX strength with +DI/-DI direction may have different later aligned responses from range controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"period": args.period, "strength_threshold": args.strength_threshold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Wilder smoothing, period and strength threshold are explicit conventions and may differ from platform rounding",
            "ADX measures historical directional strength; it does not identify trader intent or guarantee continuation",
            "DI crosses and strength states are descriptive labels, not entry, exit, stop or forecast rules",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
