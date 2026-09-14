#!/usr/bin/env python3
"""Replay BTC responses by as-of Ichimoku cloud alignment.

The cloud is handled without look-ahead: Senkou spans visible at the current
bar are the spans calculated ``displacement`` bars earlier.  The replay
classifies bullish/bearish multi-component alignment, mixed above/below-cloud
states, and inside-cloud context, then reports later candle responses.  It is
an indicator study, not a forecast, allocation, stop, or execution engine.
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


def midpoint(rows, end_index, period):
    start = end_index - period + 1
    if start < 0 or period <= 0:
        return None
    window = rows[start:end_index + 1]
    if len(window) != period:
        return None
    return (max(row["high"] for row in window) + min(row["low"] for row in window)) / 2.0


def ichimoku_at(rows, index, conversion_period, base_period, span_b_period, displacement):
    if index < displacement:
        return None
    tenkan = midpoint(rows, index, conversion_period)
    kijun = midpoint(rows, index, base_period)
    projected_index = index - displacement
    prior_tenkan = midpoint(rows, projected_index, conversion_period)
    prior_kijun = midpoint(rows, projected_index, base_period)
    span_b = midpoint(rows, projected_index, span_b_period)
    past_close = rows[projected_index]["close"] if projected_index >= 0 else None
    values = (tenkan, kijun, prior_tenkan, prior_kijun, span_b, past_close)
    if any(value is None or value <= 0 for value in values):
        return None
    span_a = (prior_tenkan + prior_kijun) / 2.0
    return {
        "tenkan": tenkan,
        "kijun": kijun,
        "span_a": span_a,
        "span_b": span_b,
        "cloud_top": max(span_a, span_b),
        "cloud_bottom": min(span_a, span_b),
        "past_close": past_close,
    }


def classify_state(close, values, cloud_buffer):
    if values is None or close is None or close <= 0:
        return "observe_only_missing_ichimoku"
    above = close > values["cloud_top"] * (1.0 + cloud_buffer)
    below = close < values["cloud_bottom"] * (1.0 - cloud_buffer)
    if not above and not below:
        return "inside_cloud"
    bullish = (above and values["tenkan"] > values["kijun"]
               and values["span_a"] >= values["span_b"]
               and close > values["past_close"])
    bearish = (below and values["tenkan"] < values["kijun"]
               and values["span_a"] <= values["span_b"]
               and close < values["past_close"])
    if bullish:
        return "bullish_alignment"
    if bearish:
        return "bearish_alignment"
    return "above_cloud_mixed" if above else "below_cloud_mixed"


def build_observations(rows, conversion_period, base_period, span_b_period,
                       displacement, cloud_buffer_bps, horizon_bars):
    if (min(conversion_period, base_period, span_b_period, displacement, horizon_bars) <= 0
            or conversion_period >= base_period or base_period >= span_b_period):
        raise ValueError("periods must be positive and ordered conversion < base < span_b")
    observations = []
    first = span_b_period + displacement - 1
    for index in range(first, len(rows) - horizon_bars):
        values = ichimoku_at(rows, index, conversion_period, base_period,
                             span_b_period, displacement)
        state = classify_state(rows[index]["close"], values, cloud_buffer_bps / 10_000.0)
        if state.startswith("observe_only"):
            continue
        future = rows[index + horizon_bars]
        path = [rows[offset]["close"] for offset in range(index + 1, index + horizon_bars + 1)]
        forward = (future["close"] / rows[index]["close"] - 1.0) * 100.0
        observations.append({
            "ts_ms": rows[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "close": rows[index]["close"],
            "state": state,
            "indicators": values,
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
    states = ("bullish_alignment", "bearish_alignment", "above_cloud_mixed",
              "below_cloud_mixed", "inside_cloud")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    return {
        "aligned_forward_windows": len(observations),
        "by_state": by_state,
        "verdict": ("ichimoku_response_reported" if len(observations) >= min_observations
                     else "observe_only_insufficient_ichimoku_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="4h")
    parser.add_argument("--days", type=float, default=730.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--conversion-period", type=int, default=9)
    parser.add_argument("--base-period", type=int, default=26)
    parser.add_argument("--span-b-period", type=int, default=52)
    parser.add_argument("--displacement", type=int, default=26)
    parser.add_argument("--cloud-buffer-bps", type=float, default=0.0)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 2 <= args.limit <= 1500
            or min(args.conversion_period, args.base_period, args.span_b_period,
                   args.displacement, args.horizon_bars) <= 0
            or args.conversion_period >= args.base_period
            or args.base_period >= args.span_b_period
            or args.cloud_buffer_bps < 0 or args.min_observations <= 0
            or args.timeout <= 0):
        parser.error("invalid Ichimoku periods, buffer, horizon or observations")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = build_observations(
        rows, args.conversion_period, args.base_period, args.span_b_period,
        args.displacement, args.cloud_buffer_bps, args.horizon_bars,
    )
    print(json.dumps({
        "strategy": "crypto_ichimoku_cloud_response_replay",
        "hypothesis": "as-of Ichimoku cloud and multi-component alignment states may have different later BTC responses",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"conversion_period": args.conversion_period,
                    "base_period": args.base_period, "span_b_period": args.span_b_period,
                    "displacement": args.displacement,
                    "cloud_buffer_bps": args.cloud_buffer_bps,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"price_bars": len(rows), "signals": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations),
        "coverage": payload.get("coverage_detail"),
        "upstream_errors": [payload["error"]] if payload.get("error") else [],
        "limitations": [
            "Senkou spans visible now are calculated from displaced historical lines; no future span is used",
            "Ichimoku periods, displacement, cloud buffer and horizon are caller-supplied sensitivity parameters",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, allocation and fills",
            "indicator alignment is descriptive and does not prove trend causality or a tradeable edge",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
