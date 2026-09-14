#!/usr/bin/env python3
"""Replay BTC responses by a point-in-time ATR volatility regime.

The replay uses a transparent simple-average true range (ATR) proxy.  Each
current ATR is classified against a trailing distribution as compressed,
ordinary, or expanded, then compared with a fixed future return and path-risk
distribution.  It is a volatility-context study, not a direction forecast,
position-sizing engine, stop-loss engine, or execution model.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
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
                and values["high"] >= values["low"] > 0):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def true_ranges(rows):
    values = [None]
    for index in range(1, len(rows)):
        row, previous_close = rows[index], rows[index - 1]["close"]
        values.append(max(row["high"] - row["low"],
                          abs(row["high"] - previous_close),
                          abs(row["low"] - previous_close)))
    return values


def atr_series(rows, period):
    if period <= 0:
        raise ValueError("period must be positive")
    ranges = true_ranges(rows)
    result = [None] * len(rows)
    for index in range(period, len(rows)):
        window = ranges[index - period + 1:index + 1]
        if len(window) == period and all(value is not None for value in window):
            result[index] = statistics.mean(window)
    return result


def percentile(values, fraction):
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower, upper = int(position), min(int(position) + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def classify_atr(current, history, low_quantile, high_quantile):
    if current is None or not history:
        return "observe_only_missing_atr", None, None
    low = percentile(history, low_quantile)
    high = percentile(history, high_quantile)
    if low is None or high is None:
        return "observe_only_missing_atr", low, high
    if current <= low:
        return "compressed", low, high
    if current >= high:
        return "expanded", low, high
    return "ordinary", low, high


def build_observations(rows, atr_period, regime_lookback, low_quantile,
                       high_quantile, horizon_bars):
    if (atr_period <= 0 or regime_lookback <= 0 or horizon_bars <= 0
            or not 0 < low_quantile < high_quantile < 1):
        raise ValueError("invalid ATR, regime, horizon or quantile parameters")
    atr = atr_series(rows, atr_period)
    observations = []
    first = atr_period + regime_lookback - 1
    for index in range(first, len(rows) - horizon_bars):
        history = [value for value in atr[index - regime_lookback:index]
                   if value is not None]
        state, low, high = classify_atr(atr[index], history, low_quantile, high_quantile)
        if state.startswith("observe_only"):
            continue
        future = rows[index + horizon_bars]
        path = [rows[offset]["close"] for offset in range(index + 1, index + horizon_bars + 1)]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "close": rows[index]["close"],
            "atr": atr[index],
            "atr_pct_of_close": atr[index] / rows[index]["close"] * 100.0,
            "regime_low": low,
            "regime_high": high,
            "state": state,
            "forward_return_pct": forward,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(path) / rows[index]["close"] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / rows[index]["close"] - 1.0) * 100.0,
        })
    return observations


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_absolute_return_pct"] for row in rows]
    minima = [row["forward_min_path_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_absolute_return_pct": statistics.mean(absolute) if absolute else None,
        "negative_forward_fraction": (sum(value < 0 for value in signed) / len(signed)
                                       if signed else None),
        "mean_forward_min_path_return_pct": statistics.mean(minima) if minima else None,
    }


def summarize(observations, min_observations):
    states = ("compressed", "ordinary", "expanded")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    qualifying = sum(bucket["observations"] for bucket in by_state.values())
    return {
        "aligned_forward_windows": len(observations),
        "by_state": by_state,
        "verdict": ("atr_regime_response_reported" if qualifying >= min_observations
                     else "observe_only_insufficient_atr_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=90.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--atr-period", type=int, default=14)
    parser.add_argument("--regime-lookback", type=int, default=96)
    parser.add_argument("--low-quantile", type=float, default=0.20)
    parser.add_argument("--high-quantile", type=float, default=0.80)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500 or args.atr_period <= 0
            or args.regime_lookback <= 0 or args.horizon_bars <= 0
            or not 0 < args.low_quantile < args.high_quantile < 1
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid ATR, regime, horizon, quantile or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.atr_period, args.regime_lookback, args.low_quantile,
        args.high_quantile, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_atr_regime_response_replay",
        "hypothesis": "compressed, ordinary and expanded point-in-time ATR states may have different later signed and absolute BTC responses",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"atr_period": args.atr_period, "regime_lookback": args.regime_lookback,
                    "low_quantile": args.low_quantile, "high_quantile": args.high_quantile,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "ATR uses a simple average true range convention rather than a universal indicator standard",
            "regime quantiles, lookback and horizon are caller-supplied sensitivity parameters",
            "row-count horizons omit missing-bar timing, fees, funding, slippage and fills",
            "volatility context does not forecast direction or implement ATR position sizing/stops",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
