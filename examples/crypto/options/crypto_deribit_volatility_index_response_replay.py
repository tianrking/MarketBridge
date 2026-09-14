#!/usr/bin/env python3
"""Compare BTC responses after high/low Deribit volatility-index states.

The falsifiable hypothesis is deliberately descriptive: a high or low public
Deribit volatility-index close may be followed by a different absolute BTC
response than ordinary index observations.  The provider index is context,
not a complete option surface, volatility forecast, option PnL or execution
instruction.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_funding_spread_response_replay import fetch, price_points


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def volatility_points(payload):
    """Return sorted (timestamp, close) points from normalized API rows."""
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close >= 0:
            points.append((timestamp, close))
    return sorted(set(points))


def forward_return(points, timestamp, horizon_bars):
    if not points or horizon_bars <= 0:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index >= len(points) or future_index >= len(points):
        return None
    baseline = points[index][1]
    future = points[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def classify_state(volatility_index, low_threshold, high_threshold):
    if volatility_index is None:
        return "observe_only_missing_volatility_index"
    if volatility_index >= high_threshold:
        return "high_deribit_volatility_index"
    if volatility_index <= low_threshold:
        return "low_deribit_volatility_index"
    return "ordinary_deribit_volatility_index"


def build_observations(volatility, prices, horizon_bars, low_threshold, high_threshold):
    rows = []
    for timestamp, value in volatility:
        future = forward_return(prices, timestamp, horizon_bars)
        rows.append({
            "ts_ms": timestamp,
            "volatility_index_close": value,
            "state": classify_state(value, low_threshold, high_threshold),
            "forward_return_pct": future,
            "absolute_forward_return_pct": abs(future) if future is not None else None,
        })
    return rows


def state_stats(rows, state):
    selected = [row for row in rows
                if row.get("state") == state and row.get("forward_return_pct") is not None]
    signed = [row["forward_return_pct"] for row in selected]
    absolute = [row["absolute_forward_return_pct"] for row in selected]
    return {
        "observations": len(selected),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(rows, min_observations, min_edge_bps):
    states = (
        "high_deribit_volatility_index",
        "ordinary_deribit_volatility_index",
        "low_deribit_volatility_index",
    )
    buckets = {state: state_stats(rows, state) for state in states}
    ordinary = buckets["ordinary_deribit_volatility_index"]["mean_absolute_forward_return_pct"]
    high = buckets["high_deribit_volatility_index"]["mean_absolute_forward_return_pct"]
    low = buckets["low_deribit_volatility_index"]["mean_absolute_forward_return_pct"]
    high_edge_bps = (high - ordinary) * 100.0 if high is not None and ordinary is not None else None
    low_edge_bps = (low - ordinary) * 100.0 if low is not None and ordinary is not None else None
    verdict = "observe_only"
    if (buckets["high_deribit_volatility_index"]["observations"] >= min_observations
            and high_edge_bps is not None and high_edge_bps >= min_edge_bps):
        verdict = "high_deribit_volatility_response_candidate"
    elif (buckets["low_deribit_volatility_index"]["observations"] >= min_observations
          and low_edge_bps is not None and low_edge_bps >= min_edge_bps):
        verdict = "low_deribit_volatility_response_candidate"
    return {
        "by_state": buckets,
        "high_minus_ordinary_absolute_response_edge_bps": high_edge_bps,
        "low_minus_ordinary_absolute_response_edge_bps": low_edge_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": verdict,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--resolution", default="3600", choices=("1", "60", "3600", "43200", "1D"))
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--price-interval", default="1h")
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--low-threshold", type=float, default=25.0)
    parser.add_argument("--high-threshold", type=float, default=75.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.horizon_bars <= 0
            or args.low_threshold < 0 or args.high_threshold <= args.low_threshold
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, edge, observation or timeout argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    volatility_payload = fetch(args.base_url, "/v1/history/volatility-index", {
        "currency": args.currency,
        "resolution": args.resolution,
        "start_ms": start_ms,
        "end_ms": now_ms,
        "limit": min(args.limit, 1_000),
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange,
        "symbol": args.price_symbol,
        "candle_type": "perp",
        "interval": args.price_interval,
        "start_ms": start_ms,
        "end_ms": now_ms,
        "limit": min(max(args.limit * 2, 100), 1_500),
    }, args.timeout)
    volatility = volatility_points(volatility_payload)
    prices = price_points(price_payload)
    observations = build_observations(
        volatility, prices, args.horizon_bars, args.low_threshold, args.high_threshold,
    )
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("volatility_index", volatility_payload), ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_deribit_volatility_index_response_replay",
        "currency": args.currency.upper(),
        "resolution": args.resolution,
        "price_exchange": args.price_exchange,
        "price_symbol": args.price_symbol,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {
            "low_threshold": args.low_threshold,
            "high_threshold": args.high_threshold,
            "horizon_bars": args.horizon_bars,
            "min_observations": args.min_observations,
        },
        "source_counts": {"volatility_index": len(volatility), "price_bars": len(prices)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations, args.min_edge_bps),
        "coverage": {"volatility_index": volatility_payload.get("coverage_detail"),
                     "price": price_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "Deribit volatility-index candles are provider context, not a complete option surface or forecast",
            "the response uses a separate public price venue and does not establish causality",
            "forward response excludes option PnL, hedging, fees, slippage and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
