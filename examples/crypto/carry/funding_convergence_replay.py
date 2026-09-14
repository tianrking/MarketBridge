#!/usr/bin/env python3
"""Replay cross-venue funding differentials from MarketBridge history.

This is the historical companion to ``funding_convergence_monitor.py``. It
aligns the latest known funding observations without filling missing values with
zero, normalizes each venue by point-in-time intervals inferred from adjacent
funding timestamps, and
reports persistence statistics. An optional explicit paper hurdle can be
subtracted from the hourly differential; it is a sensitivity input, not a
venue fee schedule. It is a research replay only: it does not simulate hedges,
fills, borrow, transfers or liquidation.
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


def numeric(value):
    return float(value) if isinstance(value, (int, float)) else None


def interval_map(payload, symbol):
    return {
        str(row.get("exchange")): int(row["funding_interval_ms"])
        for row in payload.get("funding", [])
        if str(row.get("symbol", "")).upper() == symbol.upper()
        and numeric(row.get("funding_interval_ms"))
        and numeric(row.get("funding_interval_ms")) > 0
    }


def history_points(payload):
    schedule = payload.get("funding_schedule", {})
    point_intervals = {
        int(point["funding_time_ms"]): int(point["interval_ms"])
        for point in schedule.get("points", [])
        if isinstance(point, dict)
        and isinstance(point.get("funding_time_ms"), int)
        and isinstance(point.get("interval_ms"), int)
        and point["interval_ms"] > 0
    }
    points = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        rate = numeric(row.get("close"))
        interval_ms = point_intervals.get(ts_ms)
        if isinstance(ts_ms, int) and rate is not None and interval_ms:
            points.append((ts_ms, rate, interval_ms))
    return sorted(set(points))


def aligned_spreads(series, max_age_multiplier, paper_cost_bps_per_hour=0.0):
    exchanges = sorted(series)
    timeline = sorted({point[0] for points in series.values() for point in points})
    cursors = {exchange: 0 for exchange in exchanges}
    latest = {exchange: None for exchange in exchanges}
    observations = []
    for ts_ms in timeline:
        for exchange in exchanges:
            points = series[exchange]
            while cursors[exchange] < len(points) and points[cursors[exchange]][0] <= ts_ms:
                latest[exchange] = points[cursors[exchange]]
                cursors[exchange] += 1
        if any(latest[exchange] is None for exchange in exchanges):
            continue
        if any(
            ts_ms - latest[exchange][0] > latest[exchange][2] * max_age_multiplier
            for exchange in exchanges
        ):
            continue
        hourly = {
            exchange: latest[exchange][1] / (latest[exchange][2] / 3_600_000.0)
            for exchange in exchanges
        }
        low_exchange = min(hourly, key=hourly.get)
        high_exchange = max(hourly, key=hourly.get)
        spread_bps = (hourly[high_exchange] - hourly[low_exchange]) * 10_000.0
        observations.append({
            "ts_ms": ts_ms,
            "spread_bps_per_hour": spread_bps,
            "paper_cost_bps_per_hour": paper_cost_bps_per_hour,
            "net_spread_bps_per_hour": spread_bps - paper_cost_bps_per_hour,
            "low_exchange": low_exchange,
            "high_exchange": high_exchange,
            "hourly_rates": hourly,
        })
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchanges", default="binance,bybit")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--limit", type=int, default=200)
    parser.add_argument("--min-spread-bps-per-hour", type=float, default=0.5)
    parser.add_argument("--paper-cost-bps-per-hour", type=float, default=0.0,
                        help="Explicit non-venue paper hurdle subtracted from gross spread.")
    parser.add_argument("--min-net-spread-bps-per-hour", type=float, default=0.0,
                        help="Net hourly threshold used for persistence qualification.")
    parser.add_argument("--max-age-multiplier", type=float, default=2.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    exchanges = [item.strip().lower() for item in options.exchanges.split(",") if item.strip()]
    if len(set(exchanges)) < 2:
        raise SystemExit("--exchanges must contain at least two unique venues")
    if options.days <= 0 or options.limit <= 0 or options.max_age_multiplier <= 0:
        raise SystemExit("days, limit and max-age-multiplier must be positive")
    if (options.min_spread_bps_per_hour < 0 or options.paper_cost_bps_per_hour < 0
            or options.min_net_spread_bps_per_hour < 0):
        raise SystemExit("spread thresholds and paper cost cannot be negative")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(options.days * 86_400_000)
    metadata = fetch(options.base_url, "/v1/market/perpetual-funding", {
        "symbols": options.symbol,
        "exchanges": ",".join(exchanges),
        "active_only": "true",
        "limit": 100,
    }, options.timeout)
    current_intervals = interval_map(metadata, options.symbol)

    series = {}
    coverage = {}
    errors = list(metadata.get("errors", []))
    for exchange in exchanges:
        payload = fetch(options.base_url, "/v1/history/candles", {
            "exchange": exchange,
            "symbol": options.symbol,
            "candle_type": "funding_rate",
            "start_ms": start_ms,
            "end_ms": now_ms,
            "limit": options.limit,
        }, options.timeout)
        if payload.get("error"):
            errors.append({"exchange": exchange, "error": payload["error"]})
        coverage[exchange] = payload.get("coverage_detail")
        series[exchange] = history_points(payload)

    point_in_time_intervals = {
        exchange: sorted({point[2] for point in points})
        for exchange, points in series.items()
    }
    missing_schedule = [exchange for exchange in exchanges if not series.get(exchange)]
    observations = aligned_spreads(
        series, options.max_age_multiplier, options.paper_cost_bps_per_hour
    )
    spreads = [item["spread_bps_per_hour"] for item in observations]
    net_spreads = [item["net_spread_bps_per_hour"] for item in observations]
    qualifying = [value for value in spreads if value >= options.min_spread_bps_per_hour]
    net_qualifying = [
        value for value in net_spreads if value >= options.min_net_spread_bps_per_hour
    ]
    if spreads:
        summary = {
            "observations": len(spreads),
            "qualifying_observations": len(qualifying),
            "qualifying_fraction": len(qualifying) / len(spreads),
            "net_qualifying_observations": len(net_qualifying),
            "net_qualifying_fraction": len(net_qualifying) / len(net_spreads),
            "median_spread_bps_per_hour": statistics.median(spreads),
            "max_spread_bps_per_hour": max(spreads),
            "median_net_spread_bps_per_hour": statistics.median(net_spreads),
            "max_net_spread_bps_per_hour": max(net_spreads),
            "gross_annualized_bps_proxy_from_median": statistics.median(spreads) * 24.0 * 365.0,
            "net_annualized_bps_proxy_from_median": statistics.median(net_spreads) * 24.0 * 365.0,
            "paper_cost_bps_per_hour": options.paper_cost_bps_per_hour,
            "min_net_spread_bps_per_hour": options.min_net_spread_bps_per_hour,
            "current_intervals_ms": current_intervals,
            "point_in_time_intervals_ms": point_in_time_intervals,
        }
        evidence = ["historical_overlap_available"]
        if qualifying:
            evidence.append("spread_threshold_observed")
        if net_qualifying:
            evidence.append("after_cost_spread_threshold_observed")
    else:
        summary = {
            "observations": 0,
            "current_intervals_ms": current_intervals,
            "point_in_time_intervals_ms": point_in_time_intervals,
        }
        evidence = ["no_aligned_history"]
    if missing_schedule:
        evidence.append("point_in_time_schedule_missing")
    for exchange, detail in coverage.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{exchange}_coverage_{detail['status']}")

    persistent = bool(net_spreads) and len(net_qualifying) / len(net_spreads) >= 0.5
    print(json.dumps({
        "strategy": "funding_convergence_replay",
        "symbol": options.symbol,
        "exchanges": exchanges,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": options.days},
        "summary": summary,
        "coverage": coverage,
        "filters": {
            "min_spread_bps_per_hour": options.min_spread_bps_per_hour,
            "paper_cost_bps_per_hour": options.paper_cost_bps_per_hour,
            "min_net_spread_bps_per_hour": options.min_net_spread_bps_per_hour,
        },
        "verdict": "persistent after-cost differential candidate" if persistent else "observe only",
        "evidence": evidence,
        "upstream_errors": errors,
        "execution": "research_only_no_orders",
        "limitations": [
            "the replay uses adjacent historical funding timestamps as point-in-time intervals",
            "latest-observation alignment is not a fill or hedge simulation",
            "paper cost is a user-supplied sensitivity hurdle, not a venue fee, borrow, margin or slippage estimate",
            "fees, borrow, margin, transfer latency, mark/index divergence, slippage and liquidation remain unmodeled",
        ],
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
