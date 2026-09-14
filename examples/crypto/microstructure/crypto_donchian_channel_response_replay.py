#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Donchian-channel states.

The upper and lower channel are the prior lookback high and low, excluding the
current candle.  Current closes are labelled as first bullish/bearish breaks,
persistent outside states, or inside-channel controls, then measured over a
fixed future horizon.  This is a range-state response study, not a trend
guarantee, stop rule, or execution model.
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


def donchian_series(rows, lookback_bars):
    if lookback_bars <= 0:
        return []
    series = [None] * len(rows)
    for index in range(lookback_bars, len(rows)):
        prior = rows[index - lookback_bars:index]
        upper = max(row["high"] for row in prior)
        lower = min(row["low"] for row in prior)
        close = rows[index]["close"]
        above, below = close > upper, close < lower
        previous = series[index - 1]
        if above and (previous is None or previous["state"] not in
                      {"bullish_breakout", "bullish_outside_channel"}):
            state, direction = "bullish_breakout", 1
        elif below and (previous is None or previous["state"] not in
                        {"bearish_breakout", "bearish_outside_channel"}):
            state, direction = "bearish_breakout", -1
        elif above:
            state, direction = "bullish_outside_channel", 1
        elif below:
            state, direction = "bearish_outside_channel", -1
        else:
            state, direction = "inside_channel", 0
        series[index] = {
            "upper": upper,
            "lower": lower,
            "middle": (upper + lower) / 2.0,
            "width_pct": (upper - lower) / ((upper + lower) / 2.0) * 100.0
            if upper + lower > 0 else None,
            "distance_to_upper_bps": (close / upper - 1.0) * 10_000.0,
            "distance_to_lower_bps": (close / lower - 1.0) * 10_000.0,
            "above_channel": above,
            "below_channel": below,
            "state": state,
            "direction_sign": direction,
        }
    return series


def donchian_features(rows, index, lookback_bars):
    series = donchian_series(rows, lookback_bars)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, lookback_bars, horizon_bars):
    if lookback_bars <= 0 or horizon_bars <= 0:
        raise ValueError("lookback and horizon must be positive")
    series = donchian_series(rows, lookback_bars)
    observations = []
    for index in range(lookback_bars, len(rows) - horizon_bars):
        features = series[index]
        if features is None:
            continue
        future = rows[index + horizon_bars]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        direction = features["direction_sign"]
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": features["state"],
            "features": features,
            "forward_return_pct": forward,
            "direction_aligned_return_bps": direction * forward * 100.0 if direction else None,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (
                min(row["low"] for row in rows[index + 1:index + horizon_bars + 1])
                / rows[index]["close"] - 1.0
            ) * 100.0,
            "forward_max_path_return_pct": (
                max(row["high"] for row in rows[index + 1:index + horizon_bars + 1])
                / rows[index]["close"] - 1.0
            ) * 100.0,
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
    states = ("bullish_breakout", "bearish_breakout", "bullish_outside_channel",
              "bearish_outside_channel", "inside_channel")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    breakouts = by_state["bullish_breakout"]["observations"] + by_state["bearish_breakout"]["observations"]
    sufficient = breakouts >= min_observations and by_state["inside_channel"]["observations"] >= min_observations
    return {
        "observations": len(observations),
        "breakout_observations": breakouts,
        "inside_control_observations": by_state["inside_channel"]["observations"],
        "by_state": by_state,
        "verdict": ("donchian_channel_response_reported" if sufficient
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
    parser.add_argument("--lookback-bars", type=int, default=20)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid lookback, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.lookback_bars, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_donchian_channel_response_replay",
        "hypothesis": "point-in-time Donchian range breakouts may have different later aligned responses from inside-channel controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "channel levels use prior-window OHLCV extrema and are not validated support/resistance or resting orders",
            "lookback and horizon are caller-supplied sensitivity parameters",
            "a close outside the channel is a state label, not proof of continuation, trend or execution",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
