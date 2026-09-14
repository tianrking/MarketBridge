#!/usr/bin/env python3
"""Replay BTC responses after a provider funding-interval change.

The falsifiable hypothesis is narrow: when adjacent settled funding timestamps
show a changed interval, does the next fixed price window differ from periods
where the observed interval is stable? This uses observed timestamps rather
than assuming an 8-hour schedule, and never annualizes a rate or models a
position, funding income, or execution.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_funding_spread_response_replay import fetch


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def funding_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        rate = number(row.get("close"))
        if isinstance(timestamp, int) and rate is not None:
            points.append((timestamp, rate))
    return sorted(set(points))


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def interval_points(points):
    rows = []
    for index in range(2, len(points)):
        current_ts = points[index][0]
        interval_ms = points[index][0] - points[index - 1][0]
        previous_ms = points[index - 1][0] - points[index - 2][0]
        if interval_ms <= 0 or previous_ms <= 0:
            continue
        rows.append({
            "ts_ms": current_ts,
            "interval_ms": interval_ms,
            "previous_interval_ms": previous_ms,
            "interval_hours": interval_ms / 3_600_000.0,
            "previous_interval_hours": previous_ms / 3_600_000.0,
            "state": "interval_change" if interval_ms != previous_ms else "stable_interval",
        })
    return rows


def forward_return(timestamp, prices, horizon_bars):
    if horizon_bars <= 0:
        return None
    timestamps = [point[0] for point in prices]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if index >= len(prices) or future_index >= len(prices):
        return None
    baseline, future = prices[index][1], prices[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def build_observations(funding, prices, horizon_bars):
    rows = []
    for state in interval_points(funding):
        forward = forward_return(state["ts_ms"], prices, horizon_bars)
        rows.append({**state,
                     "forward_return_pct": forward,
                     "absolute_forward_return_pct": abs(forward) if forward is not None else None})
    return rows


def summarize(rows, min_observations, min_abs_edge_bps):
    by_state = {}
    for state in ("interval_change", "stable_interval"):
        selected = [row for row in rows
                    if row["state"] == state and row["forward_return_pct"] is not None]
        signed = [row["forward_return_pct"] for row in selected]
        absolute = [abs(value) for value in signed]
        by_state[state] = {
            "observations": len(selected),
            "mean_forward_return_pct": statistics.mean(signed) if signed else None,
            "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        }
    changed = by_state["interval_change"]["mean_absolute_forward_return_pct"]
    stable = by_state["stable_interval"]["mean_absolute_forward_return_pct"]
    edge_bps = (changed - stable) * 100.0 if changed is not None and stable is not None else None
    candidate = (by_state["interval_change"]["observations"] >= min_observations
                 and by_state["stable_interval"]["observations"] >= min_observations
                 and edge_bps is not None and edge_bps >= min_abs_edge_bps)
    return {
        "by_state": by_state,
        "absolute_response_edge_bps_change_minus_stable": edge_bps,
        "min_observations": min_observations,
        "min_abs_edge_bps": min_abs_edge_bps,
        "verdict": "funding_interval_response_candidate" if candidate else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="okx")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--period", default="8h")
    parser.add_argument("--price-interval", default="1h")
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--funding-limit", type=int, default=100)
    parser.add_argument("--funding-pages", type=int, default=4)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--price-pages", type=int, default=4)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.funding_limit <= 0 or args.price_limit <= 0
            or args.funding_pages < 1 or args.funding_pages > 48
            or args.price_pages < 1 or args.price_pages > 48
            or args.horizon_bars <= 0 or args.min_observations <= 0
            or args.min_abs_edge_bps < 0 or args.timeout <= 0):
        parser.error("invalid window, limits, pages, horizon or observation argument")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    funding_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "symbol": args.symbol,
        "candle_type": "funding_rate", "interval": args.period,
        "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.funding_limit, 500), "pages": args.funding_pages,
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.price_interval,
        "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.price_limit, 1500), "pages": args.price_pages,
    }, args.timeout)
    funding = funding_points(funding_payload)
    prices = price_points(price_payload)
    rows = build_observations(funding, prices, args.horizon_bars)
    payloads = (("funding", funding_payload), ("price", price_payload))
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in payloads if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_funding_interval_change_response_replay",
        "hypothesis": "an observed funding settlement-interval change may separate later BTC response from stable-interval observations",
        "market": {"exchange": args.exchange, "price_exchange": args.price_exchange,
                   "symbol": args.symbol, "funding_period_request": args.period,
                   "price_interval": args.price_interval, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"funding_pages": args.funding_pages, "price_pages": args.price_pages,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations,
                    "min_abs_edge_bps": args.min_abs_edge_bps},
        "source_counts": {"funding_points": len(funding), "price_bars": len(prices),
                          "observations": len(rows)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations, args.min_abs_edge_bps),
        "coverage": {name: payload.get("coverage_detail") for name, payload in payloads},
        "upstream_errors": errors,
        "limitations": [
            "settlement interval is inferred from adjacent provider timestamps and is not a position ledger",
            "funding period request is a query hint; provider timestamps remain authoritative",
            "forward price response is descriptive and excludes funding income, fees, fills and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
