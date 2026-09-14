#!/usr/bin/env python3
"""Replay OHLCV responses around point-in-time Aroon states.

Aroon measures how recently the current lookback window made a high or low.
This implementation uses the current candle and a transparent inclusive
lookback, labels recent-extreme, consolidation and crossover states, then
measures a fixed future response.  It is a descriptive trend-age study, not a
forecast, entry rule or execution model.
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


def _last_extreme_offset(values, prefer_max):
    target = max(values) if prefer_max else min(values)
    for offset in range(len(values) - 1, -1, -1):
        if values[offset] == target:
            return len(values) - 1 - offset
    return None


def aroon_series(rows, period, trend_threshold=70.0, consolidation_threshold=50.0):
    """Return Aroon features from an inclusive, as-of lookback window."""
    if (period <= 0 or not 0 <= consolidation_threshold <= 100
            or not 0 <= trend_threshold <= 100):
        return []
    series = [None] * len(rows)
    for index in range(period - 1, len(rows)):
        window = rows[index - period + 1:index + 1]
        high_offset = _last_extreme_offset([row["high"] for row in window], True)
        low_offset = _last_extreme_offset([row["low"] for row in window], False)
        up = (period - high_offset) / period * 100.0
        down = (period - low_offset) / period * 100.0
        oscillator = up - down
        direction = 1 if oscillator > 0 else -1 if oscillator < 0 else 0
        if up >= trend_threshold and down <= 100.0 - trend_threshold:
            state = "bullish_recent_extreme"
        elif down >= trend_threshold and up <= 100.0 - trend_threshold:
            state = "bearish_recent_extreme"
        elif up < consolidation_threshold and down < consolidation_threshold:
            state = "consolidation"
        else:
            state = "balanced"
        prior = series[index - 1]
        event = None
        if prior is not None and direction != 0 and prior["direction_sign"] != 0:
            if direction > 0 and prior["direction_sign"] < 0:
                event = "bullish_aroon_cross"
            elif direction < 0 and prior["direction_sign"] > 0:
                event = "bearish_aroon_cross"
        series[index] = {
            "aroon_up": up,
            "aroon_down": down,
            "oscillator": oscillator,
            "high_bars_since": high_offset,
            "low_bars_since": low_offset,
            "direction_sign": direction,
            "trend_threshold": trend_threshold,
            "consolidation_threshold": consolidation_threshold,
            "state": state,
            "event": event,
        }
    return series


def aroon_features(rows, index, period, trend_threshold=70.0, consolidation_threshold=50.0):
    series = aroon_series(rows, period, trend_threshold, consolidation_threshold)
    return series[index] if 0 <= index < len(series) else None


def build_observations(rows, period, trend_threshold, consolidation_threshold, horizon_bars):
    if (period <= 0 or horizon_bars <= 0 or not 0 <= consolidation_threshold <= 100
            or not 0 <= trend_threshold <= 100):
        raise ValueError("invalid Aroon period, thresholds or horizon")
    series = aroon_series(rows, period, trend_threshold, consolidation_threshold)
    observations = []
    for index in range(period - 1, len(rows) - horizon_bars):
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
    states = ("bullish_recent_extreme", "bearish_recent_extreme", "consolidation", "balanced")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    events = {
        event: bucket_stats([row for row in observations if row["event"] == event])
        for event in ("bullish_aroon_cross", "bearish_aroon_cross")
    }
    extreme = by_state["bullish_recent_extreme"]["observations"] + by_state["bearish_recent_extreme"]["observations"]
    control = by_state["consolidation"]["observations"]
    sufficient = extreme >= min_observations and control >= min_observations
    return {
        "observations": len(observations),
        "recent_extreme_observations": extreme,
        "consolidation_control_observations": control,
        "aroon_cross_observations": sum(bucket["observations"] for bucket in events.values()),
        "by_state": by_state,
        "by_event": events,
        "verdict": ("aroon_response_reported" if sufficient
                     else "observe_only_insufficient_extreme_or_consolidation_control"),
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
    parser.add_argument("--trend-threshold", type=float, default=70.0)
    parser.add_argument("--consolidation-threshold", type=float, default=50.0)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.period <= 0
            or not 0 <= args.consolidation_threshold <= 100
            or not 0 <= args.trend_threshold <= 100 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid period, thresholds, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval, "start_ms": start_ms,
        "end_ms": end_ms, "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(rows, args.period, args.trend_threshold,
                                      args.consolidation_threshold, args.horizon_bars)
    print(json.dumps({
        "strategy": "crypto_aroon_response_replay",
        "hypothesis": "point-in-time recency of highs and lows may separate later aligned responses from consolidation windows",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"period": args.period, "trend_threshold": args.trend_threshold,
                    "consolidation_threshold": args.consolidation_threshold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Aroon uses inclusive OHLC extrema; ties select the most recent occurrence and platform conventions may differ",
            "trend and consolidation thresholds are caller-supplied sensitivity parameters",
            "recent-extreme and crossover labels are descriptive, not entry, exit, stop or forecast rules",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
            "overlapping windows do not establish causality or an executable trading edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
