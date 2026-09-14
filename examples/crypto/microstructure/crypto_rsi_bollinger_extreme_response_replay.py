#!/usr/bin/env python3
"""Replay responses after RSI and Bollinger extreme confluence states.

The replay uses a documented simple-average RSI and close-only Bollinger
Bands.  It separates joint overbought/oversold states from one-indicator
extremes and ordinary observations, then reports later signed, absolute and
path responses.  It is an indicator study, not an automatic reversal signal
or execution model.
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
        timestamp, close = row.get("open_time_ms"), number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            rows.append({"ts_ms": timestamp, "close": close})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def simple_rsi(closes, index, period):
    start = index - period + 1
    if period <= 0 or start < 1 or index >= len(closes):
        return None
    changes = [closes[offset] - closes[offset - 1] for offset in range(start, index + 1)]
    if len(changes) != period:
        return None
    gains = [change for change in changes if change > 0]
    losses = [-change for change in changes if change < 0]
    average_gain = sum(gains) / period
    average_loss = sum(losses) / period
    if average_loss == 0:
        return 100.0 if average_gain > 0 else 50.0
    relative_strength = average_gain / average_loss
    return 100.0 - 100.0 / (1.0 + relative_strength)


def bollinger(closes, index, period, deviations):
    start = index - period + 1
    if period <= 0 or start < 0 or index >= len(closes):
        return None
    window = closes[start:index + 1]
    if len(window) != period:
        return None
    middle = statistics.mean(window)
    spread = statistics.pstdev(window) * deviations
    return {"middle": middle, "upper": middle + spread, "lower": middle - spread,
            "bandwidth_pct": (2.0 * spread / middle * 100.0) if middle > 0 else None}


def classify_state(rsi, bands, close, overbought, oversold):
    if rsi is None or bands is None or close is None:
        return "observe_only_missing_indicators"
    rsi_overbought, rsi_oversold = rsi >= overbought, rsi <= oversold
    band_above, band_below = close > bands["upper"], close < bands["lower"]
    if rsi_overbought and band_above:
        return "overbought_confluence"
    if rsi_oversold and band_below:
        return "oversold_confluence"
    if rsi_overbought or rsi_oversold:
        return "rsi_extreme_only"
    if band_above or band_below:
        return "band_extreme_only"
    return "ordinary"


def build_observations(rows, rsi_period, band_period, deviations, overbought,
                       oversold, horizon_bars):
    if (min(rsi_period, band_period, horizon_bars) <= 0 or deviations <= 0
            or not 0 < oversold < overbought < 100):
        raise ValueError("invalid RSI, Bollinger, threshold or horizon parameters")
    closes = [row["close"] for row in rows]
    first = max(rsi_period, band_period)
    observations = []
    for index in range(first, len(rows) - horizon_bars):
        rsi = simple_rsi(closes, index, rsi_period)
        bands = bollinger(closes, index, band_period, deviations)
        state = classify_state(rsi, bands, closes[index], overbought, oversold)
        if state.startswith("observe_only"):
            continue
        future = rows[index + horizon_bars]
        path = [rows[offset]["close"] for offset in range(index + 1, index + horizon_bars + 1)]
        forward = (future["close"] / closes[index] - 1.0) * 100.0
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "close": closes[index],
            "rsi": rsi,
            "bands": bands,
            "state": state,
            "forward_return_pct": forward,
            "forward_absolute_return_pct": abs(forward),
            "forward_min_path_return_pct": (min(path) / closes[index] - 1.0) * 100.0,
            "forward_max_path_return_pct": (max(path) / closes[index] - 1.0) * 100.0,
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
    states = ("overbought_confluence", "oversold_confluence", "rsi_extreme_only",
              "band_extreme_only", "ordinary")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    return {
        "aligned_forward_windows": len(observations),
        "by_state": by_state,
        "verdict": ("rsi_bollinger_response_reported" if len(observations) >= min_observations
                     else "observe_only_insufficient_rsi_bollinger_observations"),
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
    parser.add_argument("--rsi-period", type=int, default=14)
    parser.add_argument("--band-period", type=int, default=20)
    parser.add_argument("--deviations", type=float, default=2.0)
    parser.add_argument("--overbought", type=float, default=70.0)
    parser.add_argument("--oversold", type=float, default=30.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500
            or min(args.rsi_period, args.band_period, args.horizon_bars) <= 0
            or args.deviations <= 0 or not 0 < args.oversold < args.overbought < 100
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid RSI, Bollinger, threshold, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.rsi_period, args.band_period, args.deviations,
        args.overbought, args.oversold, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_rsi_bollinger_extreme_response_replay",
        "hypothesis": "joint RSI and Bollinger extremes may have different later BTC responses from one-indicator and ordinary states",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"rsi_period": args.rsi_period, "band_period": args.band_period,
                    "deviations": args.deviations, "overbought": args.overbought,
                    "oversold": args.oversold, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "RSI uses a documented simple-average convention, not Wilder smoothing",
            "Bollinger bands are close-only population-standard-deviation bands",
            "thresholds and horizon are caller-supplied sensitivity parameters; extremes can persist in trends",
            "row-count horizons omit missing-bar timing, fees, funding, slippage and fills",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
