#!/usr/bin/env python3
"""Replay perp premium-index and funding-rate divergence responses.

The falsifiable hypothesis is that a material disagreement between Binance's
premium-index series and the latest funding observation is followed by a
different fixed-horizon perp response than ordinary aligned observations.
Premium index is a market-data context value, not a funding cash-flow claim,
and this example never constructs a hedge or places an order.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_funding_spread_response_replay import fetch, funding_points, price_points


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def premium_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        premium = number(row.get("close"))
        if isinstance(timestamp, int) and premium is not None:
            points.append((timestamp, premium))
    return sorted(set(points))


def asof_funding(points, timestamp, max_age_ms):
    if not points:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_right(timestamps, timestamp) - 1
    if index < 0:
        return None
    point = points[index]
    if timestamp - point[0] > max_age_ms:
        return None
    return point


def forward_return(points, timestamp, horizon_bars):
    if horizon_bars <= 0 or not points:
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


def classify_state(premium_bps, funding_bps, premium_threshold_bps, funding_threshold_bps):
    if premium_bps is None or funding_bps is None:
        return "observe_only_missing_alignment"
    if premium_bps >= premium_threshold_bps and funding_bps <= -funding_threshold_bps:
        return "positive_premium_negative_funding_divergence"
    if premium_bps <= -premium_threshold_bps and funding_bps >= funding_threshold_bps:
        return "negative_premium_positive_funding_divergence"
    if premium_bps >= premium_threshold_bps and funding_bps >= funding_threshold_bps:
        return "aligned_positive_pressure"
    if premium_bps <= -premium_threshold_bps and funding_bps <= -funding_threshold_bps:
        return "aligned_negative_pressure"
    return "ordinary_premium_funding"


def observations(premium, funding, prices, horizon_bars, max_age_ms,
                 premium_threshold_bps, funding_threshold_bps):
    rows = []
    for timestamp, premium_rate in premium:
        funding_point = asof_funding(funding, timestamp, max_age_ms)
        funding_rate = funding_point[1] if funding_point else None
        future = forward_return(prices, timestamp, horizon_bars)
        premium_bps = premium_rate * 10_000.0
        funding_bps = funding_rate * 10_000.0 if funding_rate is not None else None
        rows.append({
            "ts_ms": timestamp,
            "premium_index_bps": premium_bps,
            "funding_rate_bps": funding_bps,
            "funding_ts_ms": funding_point[0] if funding_point else None,
            "state": classify_state(
                premium_bps, funding_bps, premium_threshold_bps, funding_threshold_bps
            ),
            "forward_return_pct": future,
            "absolute_forward_return_pct": abs(future) if future is not None else None,
        })
    return rows


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows if row["forward_return_pct"] is not None]
    absolute = [row["absolute_forward_return_pct"] for row in rows
                if row["absolute_forward_return_pct"] is not None]
    return {
        "observations": len(rows),
        "forward_observations": len(signed),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(rows, min_observations, min_edge_bps):
    divergence = [row for row in rows if "divergence" in row["state"] and row["forward_return_pct"] is not None]
    ordinary = [row for row in rows if row["state"] == "ordinary_premium_funding"
                and row["forward_return_pct"] is not None]
    divergence_stats = bucket_stats(divergence)
    ordinary_stats = bucket_stats(ordinary)
    divergence_abs = divergence_stats["mean_absolute_forward_return_pct"]
    ordinary_abs = ordinary_stats["mean_absolute_forward_return_pct"]
    edge_bps = ((divergence_abs - ordinary_abs) * 100.0
                if divergence_abs is not None and ordinary_abs is not None else None)
    return {
        "divergence": divergence_stats,
        "ordinary": ordinary_stats,
        "absolute_response_edge_bps": edge_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": "premium_funding_divergence_response_candidate"
        if len(divergence) >= min_observations and edge_bps is not None and edge_bps >= min_edge_bps
        else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--max-funding-age-bars", type=float, default=12.0)
    parser.add_argument("--premium-threshold-bps", type=float, default=1.0)
    parser.add_argument("--funding-threshold-bps", type=float, default=1.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.horizon_bars <= 0
            or args.max_funding_age_bars <= 0 or args.premium_threshold_bps < 0
            or args.funding_threshold_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, observation or timeout argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"exchange": args.exchange, "symbol": args.symbol,
              "start_ms": start_ms, "end_ms": now_ms, "limit": min(args.limit, 1500)}
    premium_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "candle_type": "premiumIndex", "interval": args.interval,
    }, args.timeout)
    funding_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "candle_type": "funding_rate", "interval": "8h",
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "candle_type": "perp", "interval": args.interval,
    }, args.timeout)
    premium = premium_points(premium_payload)
    funding = funding_points(funding_payload)
    prices = price_points(price_payload)
    interval_ms = max(1, int((args.interval.endswith("m") and float(args.interval[:-1]) * 60_000)
                              or (args.interval.endswith("h") and float(args.interval[:-1]) * 3_600_000)
                              or 3_600_000))
    rows = observations(
        premium, funding, prices, args.horizon_bars,
        int(args.max_funding_age_bars * interval_ms),
        args.premium_threshold_bps, args.funding_threshold_bps,
    )
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("premium_index", premium_payload),
                                    ("funding_rate", funding_payload),
                                    ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_premium_funding_response_replay",
        "symbol": args.symbol, "venue": args.exchange, "interval": args.interval,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"premium_threshold_bps": args.premium_threshold_bps,
                    "funding_threshold_bps": args.funding_threshold_bps,
                    "max_funding_age_bars": args.max_funding_age_bars,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"premium_index": len(premium), "funding_rate": len(funding),
                           "price_bars": len(prices)},
        "observations": rows, "summary": summarize(rows, args.min_observations, args.min_edge_bps),
        "coverage": {"premium_index": premium_payload.get("coverage_detail"),
                     "funding_rate": funding_payload.get("coverage_detail"),
                     "price": price_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "premium index is an exchange-derived market-data series, not a guaranteed executable spread",
            "funding observations are carried backward only within a bounded age window",
            "forward response excludes fees, funding cash flow, slippage, hedge PnL and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
