#!/usr/bin/env python3
"""Replay the price response after an extreme cross-venue funding spread.

The falsifiable hypothesis is deliberately narrower than a funding-arbitrage
trade: when two venues show a large, fresh annualized funding differential, is
the next fixed price window's absolute BTC movement different from ordinary
funding-spread observations?  A spread is a carry-context observation, not a
directional price forecast or an executable hedge.
"""

import argparse
import bisect
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


YEAR_MS = 365.0 * 24.0 * 60.0 * 60.0 * 1000.0


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def funding_points(payload):
    schedule = payload.get("funding_schedule") or {}
    intervals = {
        point.get("funding_time_ms"): point.get("interval_ms")
        for point in schedule.get("points", [])
        if isinstance(point, dict)
        and isinstance(point.get("funding_time_ms"), int)
        and isinstance(point.get("interval_ms"), int)
        and point["interval_ms"] > 0
    }
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        rate = number(row.get("close"))
        interval_ms = intervals.get(timestamp)
        if isinstance(timestamp, int) and rate is not None and interval_ms:
            points.append((timestamp, rate, interval_ms))
    return sorted(set(points))


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def asof_funding(points, timestamp, max_age_multiplier):
    if not points:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_right(timestamps, timestamp) - 1
    if index < 0:
        return None
    point = points[index]
    if timestamp - point[0] > point[2] * max_age_multiplier:
        return None
    return point


def forward_price(points, timestamp, horizon_bars):
    if not points or horizon_bars <= 0:
        return None
    timestamps = [point[0] for point in points]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index >= len(points) or future_index >= len(points):
        return None
    baseline_ts, baseline = points[index]
    future_ts, future = points[future_index]
    if baseline <= 0 or future <= 0:
        return None
    return {
        "baseline_ts_ms": baseline_ts,
        "forward_ts_ms": future_ts,
        "forward_return_pct": (future / baseline - 1.0) * 100.0,
    }


def annualized_bps(rate, interval_ms):
    if interval_ms is None or interval_ms <= 0:
        return None
    return rate * YEAR_MS / interval_ms * 10_000.0


def spread_observations(series_a, series_b, prices, horizon_bars,
                        min_abs_spread_bps_per_year, shock_bps_per_year,
                        max_age_multiplier):
    timeline = sorted({point[0] for point in series_a} | {point[0] for point in series_b})
    observations = []
    previous_spread = None
    for timestamp in timeline:
        point_a = asof_funding(series_a, timestamp, max_age_multiplier)
        point_b = asof_funding(series_b, timestamp, max_age_multiplier)
        if point_a is None or point_b is None:
            continue
        annual_a = annualized_bps(point_a[1], point_a[2])
        annual_b = annualized_bps(point_b[1], point_b[2])
        if annual_a is None or annual_b is None:
            continue
        spread = annual_a - annual_b
        change = None if previous_spread is None else spread - previous_spread
        price = forward_price(prices, timestamp, horizon_bars)
        previous_spread = spread
        if price is None:
            continue
        extreme = abs(spread) >= min_abs_spread_bps_per_year
        shocked = (shock_bps_per_year <= 0.0 or
                   (change is not None and abs(change) >= shock_bps_per_year))
        observations.append({
            "ts_ms": timestamp,
            "price_baseline_ts_ms": price["baseline_ts_ms"],
            "forward_ts_ms": price["forward_ts_ms"],
            "venue_a_annualized_funding_bps": annual_a,
            "venue_b_annualized_funding_bps": annual_b,
            "spread_bps_per_year": spread,
            "spread_change_bps_per_year": change,
            "absolute_forward_return_pct": abs(price["forward_return_pct"]),
            "forward_return_pct": price["forward_return_pct"],
            "state": "extreme_spread" if extreme and shocked else "ordinary_spread",
        })
    return observations


def bucket_stats(rows):
    absolute_returns = [row["absolute_forward_return_pct"] * 100.0 for row in rows]
    signed_returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_absolute_forward_return_bps": statistics.mean(absolute_returns) if absolute_returns else None,
        "median_absolute_forward_return_bps": statistics.median(absolute_returns) if absolute_returns else None,
        "mean_forward_return_pct": statistics.mean(signed_returns) if signed_returns else None,
    }


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    extreme = [row for row in observations if row["state"] == "extreme_spread"]
    ordinary = [row for row in observations if row["state"] == "ordinary_spread"]
    extreme_stats = bucket_stats(extreme)
    ordinary_stats = bucket_stats(ordinary)
    extreme_mean = extreme_stats["mean_absolute_forward_return_bps"]
    ordinary_mean = ordinary_stats["mean_absolute_forward_return_bps"]
    edge = (extreme_mean - ordinary_mean
            if extreme_mean is not None and ordinary_mean is not None else None)
    adjusted = edge - paper_cost_bps if edge is not None else None
    candidate = (len(extreme) >= min_observations and edge is not None
                 and adjusted >= min_edge_bps)
    return {
        "extreme_spread": extreme_stats,
        "ordinary_spread": ordinary_stats,
        "absolute_response_edge_bps": edge,
        "paper_cost_bps": paper_cost_bps,
        "cost_adjusted_absolute_response_edge_bps": adjusted,
        "verdict": "funding_spread_stress_response_candidate" if candidate else "observe_only",
        "evidence": [
            "fresh_annualized_cross_venue_spread_and_forward_price_available"
            if observations else "no_aligned_spread_forward_windows",
            "extreme_spread_response_above_paper_hurdle" if candidate
            else "response_below_hurdle_or_insufficient_control",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange-a", default="binance")
    parser.add_argument("--exchange-b", default="bybit")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--funding-limit", type=int, default=500)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-abs-spread-bps-per-year", type=float, default=1000.0)
    parser.add_argument("--shock-bps-per-year", type=float, default=0.0)
    parser.add_argument("--max-age-multiplier", type=float, default=1.5)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.exchange_a.lower() == args.exchange_b.lower() or args.days <= 0
            or args.funding_limit <= 0 or args.price_limit <= 0 or args.horizon_bars <= 0
            or args.min_abs_spread_bps_per_year < 0 or args.shock_bps_per_year < 0
            or args.max_age_multiplier <= 0 or args.paper_cost_bps < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid venues, windows, spread, cost or observation arguments")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "start_ms": start_ms, "end_ms": now_ms}
    series = {}
    coverage = {}
    errors = []
    for name, exchange in (("venue_a", args.exchange_a), ("venue_b", args.exchange_b)):
        payload = fetch(args.base_url, "/v1/history/candles", {
            **common, "exchange": exchange, "candle_type": "funding_rate",
            "limit": min(args.funding_limit, 500),
        }, args.timeout)
        series[name] = funding_points(payload)
        coverage[name] = payload.get("coverage_detail")
        if payload.get("error"):
            errors.append({"source": name, "error": payload["error"]})
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.price_exchange, "market": args.market,
        "interval": args.interval, "limit": min(args.price_limit, 1000),
    }, args.timeout)
    prices = price_points(price_payload)
    coverage["price"] = price_payload.get("coverage_detail")
    if price_payload.get("error"):
        errors.append({"source": "price", "error": price_payload["error"]})

    observations = spread_observations(
        series["venue_a"], series["venue_b"], prices, args.horizon_bars,
        args.min_abs_spread_bps_per_year, args.shock_bps_per_year,
        args.max_age_multiplier,
    )
    summary = summarize(observations, args.min_observations,
                        args.paper_cost_bps, args.min_edge_bps)
    evidence = list(summary.pop("evidence"))
    for source, detail in coverage.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{source}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_funding_spread_response_replay",
        "symbol": args.symbol,
        "venues": {"funding_a": args.exchange_a, "funding_b": args.exchange_b,
                    "price": args.price_exchange},
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {
            "horizon_bars": args.horizon_bars,
            "min_abs_spread_bps_per_year": args.min_abs_spread_bps_per_year,
            "shock_bps_per_year": args.shock_bps_per_year,
            "max_age_multiplier": args.max_age_multiplier,
            "paper_cost_bps": args.paper_cost_bps,
            "min_edge_bps": args.min_edge_bps,
            "min_observations": args.min_observations,
        },
        "source_counts": {"funding_a": len(series["venue_a"]),
                          "funding_b": len(series["venue_b"]), "price_bars": len(prices)},
        "observations": observations,
        "summary": summary,
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "annualized funding spread is a point-in-time carry-context proxy, not realized funding income",
            "as-of alignment uses provider intervals and excludes stale or missing funding values",
            "absolute BTC movement is a response distribution, not a directional forecast",
            "paper cost is a sensitivity hurdle, not venue fees, borrow, margin, transfer or slippage",
            "no prefunded inventory, hedge, liquidation or order execution is modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
