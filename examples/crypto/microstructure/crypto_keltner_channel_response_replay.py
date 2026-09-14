#!/usr/bin/env python3
"""Replay OHLCV responses around an explicit Keltner-channel proxy.

The channel uses an EMA of close and a trailing simple true-range average as
the ATR width.  A current close crossing outside the as-of upper/lower channel
is separated from persistent outside and inside states, then measured over a
fixed future candle horizon.  This is a technical-context response study, not
a breakout, reversal, stop, or execution rule.
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


def true_range(row, previous_close):
    if previous_close is None or previous_close <= 0:
        return row["high"] - row["low"]
    return max(row["high"] - row["low"],
               abs(row["high"] - previous_close),
               abs(row["low"] - previous_close))


def ema_values(values, period):
    if period <= 0 or not values:
        return []
    alpha = 2.0 / (period + 1.0)
    result = [values[0]]
    for value in values[1:]:
        result.append(alpha * value + (1.0 - alpha) * result[-1])
    return result


def channel_features(rows, index, ema_period, atr_period, multiplier):
    if index < 0 or index >= len(rows):
        return None
    series = channel_series(rows, ema_period, atr_period, multiplier)
    return series[index] if index < len(series) else None


def channel_series(rows, ema_period, atr_period, multiplier):
    if ema_period <= 0 or atr_period <= 0 or multiplier <= 0 or not rows:
        return []
    closes = [row["close"] for row in rows]
    emas = ema_values(closes, ema_period)
    true_ranges = [true_range(row, rows[offset - 1]["close"] if offset else None)
                   for offset, row in enumerate(rows)]
    series = [None] * len(rows)
    for index in range(1, len(rows)):
        if index + 1 < atr_period:
            continue
        atr = statistics.mean(true_ranges[index - atr_period + 1:index + 1])
        if atr <= 0:
            continue
        ema = emas[index]
        upper, lower = ema + multiplier * atr, ema - multiplier * atr
        close = rows[index]["close"]
        above, below = close > upper, close < lower
        previous = series[index - 1]
        if above and (previous is None or not previous["above_channel"]):
            state, direction = "bullish_breakout", 1
        elif below and (previous is None or not previous["below_channel"]):
            state, direction = "bearish_breakout", -1
        elif above:
            state, direction = "bullish_outside_channel", 1
        elif below:
            state, direction = "bearish_outside_channel", -1
        else:
            state, direction = "inside_channel", 0
        series[index] = {
            "ema": ema,
            "atr": atr,
            "upper": upper,
            "lower": lower,
            "width_pct": (upper - lower) / ema * 100.0 if ema > 0 else None,
            "position_atr": (close - ema) / atr,
            "above_channel": above,
            "below_channel": below,
            "state": state,
            "direction_sign": direction,
        }
    return series


def build_observations(rows, ema_period, atr_period, multiplier, horizon_bars):
    if ema_period <= 0 or atr_period <= 0 or multiplier <= 0 or horizon_bars <= 0:
        raise ValueError("invalid EMA, ATR, multiplier or horizon arguments")
    observations = []
    features_series = channel_series(rows, ema_period, atr_period, multiplier)
    for index in range(max(1, atr_period - 1), len(rows) - horizon_bars):
        features = features_series[index] if index < len(features_series) else None
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
        "verdict": ("keltner_channel_response_reported" if sufficient
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
    parser.add_argument("--ema-period", type=int, default=20)
    parser.add_argument("--atr-period", type=int, default=10)
    parser.add_argument("--atr-multiplier", type=float, default=2.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.ema_period <= 0
            or args.atr_period <= 0 or args.atr_multiplier <= 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid EMA, ATR, multiplier, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.ema_period, args.atr_period,
                                       args.atr_multiplier, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_keltner_channel_response_replay",
        "hypothesis": "as-of Keltner channel breakouts may have different later aligned responses from inside-channel controls",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"ema_period": args.ema_period, "atr_period": args.atr_period,
                    "atr_multiplier": args.atr_multiplier, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Keltner channels use an explicit EMA and simple trailing true-range ATR proxy, not a platform-specific indicator implementation",
            "breakout, multiplier, periods and horizon are caller-supplied sensitivity parameters",
            "current close crossing a channel is an OHLCV state label, not proof of trend continuation or reversal",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
