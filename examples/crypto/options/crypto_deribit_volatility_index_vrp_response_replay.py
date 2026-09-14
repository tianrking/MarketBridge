#!/usr/bin/env python3
"""Compare BTC responses after Deribit volatility-index minus RV states.

The falsifiable hypothesis is that a positive public volatility-index minus
realized-volatility spread, or a negative spread, may be followed by a
different absolute BTC response than aligned observations.  It is a regime
diagnostic only: the index is not a complete surface and no short-volatility,
hedge or execution model is included.
"""

import argparse
import bisect
import json
import math
import statistics
import time

from crypto_deribit_volatility_index_response_replay import volatility_points
from crypto_funding_spread_response_replay import fetch, price_points


def interval_minutes(interval):
    if not isinstance(interval, str) or len(interval) < 2:
        return None
    units = {"m": 1, "h": 60, "d": 1440}
    try:
        return int(interval[:-1]) * units[interval[-1].lower()]
    except (KeyError, ValueError):
        return None


def realized_volatility_at(prices, timestamp, rv_bars, interval):
    minutes = interval_minutes(interval)
    if minutes is None or rv_bars <= 1 or not prices:
        return None
    timestamps = [point[0] for point in prices]
    index = bisect.bisect_right(timestamps, timestamp) - 1
    if index < rv_bars or index < 0:
        return None
    closes = [point[1] for point in prices[index - rv_bars:index + 1]]
    if len(closes) != rv_bars + 1 or any(close <= 0 for close in closes):
        return None
    timestamps_used = timestamps[index - rv_bars:index + 1]
    expected_step_ms = minutes * 60_000
    if any(current - previous != expected_step_ms
           for previous, current in zip(timestamps_used, timestamps_used[1:])):
        return None
    returns = [math.log(current / previous) for previous, current in zip(closes, closes[1:])]
    periods_per_year = 365.0 * 24.0 * 60.0 / minutes
    return statistics.pstdev(returns) * math.sqrt(periods_per_year) * 100.0


def forward_return(prices, timestamp, horizon_bars):
    if not prices or horizon_bars <= 0:
        return None
    timestamps = [point[0] for point in prices]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index < 0 or future_index >= len(prices):
        return None
    baseline = prices[index][1]
    future = prices[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def classify_vrp(spread, threshold):
    if spread is None:
        return "observe_only_missing_index_or_rv"
    if spread >= threshold:
        return "volatility_index_premium"
    if spread <= -threshold:
        return "realized_volatility_above_index"
    return "volatility_index_and_rv_aligned"


def build_observations(volatility, prices, rv_bars, price_interval, horizon_bars, threshold):
    rows = []
    for timestamp, index_value in volatility:
        realized = realized_volatility_at(prices, timestamp, rv_bars, price_interval)
        spread = index_value - realized if realized is not None else None
        future = forward_return(prices, timestamp, horizon_bars)
        rows.append({
            "ts_ms": timestamp,
            "volatility_index_close": index_value,
            "realized_volatility_pct": realized,
            "index_minus_rv_points": spread,
            "state": classify_vrp(spread, threshold),
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
        "volatility_index_premium",
        "volatility_index_and_rv_aligned",
        "realized_volatility_above_index",
    )
    buckets = {state: state_stats(rows, state) for state in states}
    aligned = buckets["volatility_index_and_rv_aligned"]["mean_absolute_forward_return_pct"]
    premium = buckets["volatility_index_premium"]["mean_absolute_forward_return_pct"]
    realized_high = buckets["realized_volatility_above_index"]["mean_absolute_forward_return_pct"]
    premium_edge = ((premium - aligned) * 100.0
                    if premium is not None and aligned is not None else None)
    realized_high_edge = ((realized_high - aligned) * 100.0
                          if realized_high is not None and aligned is not None else None)
    verdict = "observe_only"
    if (buckets["volatility_index_premium"]["observations"] >= min_observations
            and premium_edge is not None and premium_edge >= min_edge_bps):
        verdict = "volatility_index_premium_response_candidate"
    elif (buckets["realized_volatility_above_index"]["observations"] >= min_observations
          and realized_high_edge is not None and realized_high_edge >= min_edge_bps):
        verdict = "realized_volatility_above_index_response_candidate"
    return {
        "by_state": buckets,
        "premium_minus_aligned_absolute_response_edge_bps": premium_edge,
        "realized_above_index_minus_aligned_absolute_response_edge_bps": realized_high_edge,
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
    parser.add_argument("--rv-bars", type=int, default=24)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    interval = interval_minutes(args.price_interval)
    if (args.days <= 0 or args.rv_bars <= 1 or args.horizon_bars <= 0 or interval is None
            or args.vrp_threshold < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, interval, RV bars, horizon or threshold argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    volatility_payload = fetch(args.base_url, "/v1/history/volatility-index", {
        "currency": args.currency,
        "resolution": args.resolution,
        "start_ms": start_ms,
        "end_ms": now_ms,
        "limit": 1_000,
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange,
        "symbol": args.price_symbol,
        "candle_type": "perp",
        "interval": args.price_interval,
        "start_ms": max(0, start_ms - args.rv_bars * interval * 60_000),
        "end_ms": now_ms,
        "limit": min(max(int(args.days * 24 * 60 / interval) + args.rv_bars + args.horizon_bars + 10, 100), 1_500),
    }, args.timeout)
    volatility = volatility_points(volatility_payload)
    prices = price_points(price_payload)
    observations = build_observations(
        volatility, prices, args.rv_bars, args.price_interval,
        args.horizon_bars, args.vrp_threshold,
    )
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("volatility_index", volatility_payload), ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_deribit_volatility_index_vrp_response_replay",
        "currency": args.currency.upper(),
        "resolution": args.resolution,
        "price_exchange": args.price_exchange,
        "price_symbol": args.price_symbol,
        "price_interval": args.price_interval,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {
            "rv_bars": args.rv_bars,
            "horizon_bars": args.horizon_bars,
            "vrp_threshold": args.vrp_threshold,
            "min_observations": args.min_observations,
        },
        "source_counts": {"volatility_index": len(volatility), "price_bars": len(prices)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations, args.min_edge_bps),
        "coverage": {"volatility_index": volatility_payload.get("coverage_detail"),
                     "price": price_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "volatility index and close-to-close RV use different constructions and horizons",
            "the spread is provider context and does not model a volatility position, hedge or option PnL",
            "forward response uses a separate public price history and does not establish causality",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
