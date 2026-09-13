#!/usr/bin/env python3
"""Replay cross-sectional funding-rate crowding across crypto assets.

The falsifiable hypothesis is deliberately descriptive: at a timestamp where
funding dispersion is wide, do the assets with the lowest funding have a
different next-window return from the assets with the highest funding?  The
result is a cross-sectional observation, not a long/short instruction, funding
income estimate or hedge PnL.
"""

import argparse
import bisect
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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def funding_points(payload):
    intervals = {
        point.get("funding_time_ms"): point.get("interval_ms")
        for point in payload.get("funding_schedule", {}).get("points", [])
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


def aligned_price_points(series):
    symbols = sorted(series)
    if not symbols:
        return []
    by_symbol = {symbol: dict(points) for symbol, points in series.items()}
    timestamps = sorted(set.intersection(*(set(points) for points in by_symbol.values())))
    return [(timestamp, {symbol: by_symbol[symbol][timestamp] for symbol in symbols})
            for timestamp in timestamps]


def _asof_funding(points, timestamp, max_age_multiplier):
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


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def cross_section_observations(funding_series, price_series, horizon_bars, top_k,
                              min_dispersion_bps, max_age_multiplier):
    aligned = aligned_price_points(price_series)
    if horizon_bars <= 0 or top_k <= 0 or min_dispersion_bps < 0 or max_age_multiplier <= 0:
        return []
    observations = []
    for index in range(len(aligned) - horizon_bars):
        timestamp, current = aligned[index]
        forward_timestamp, future = aligned[index + horizon_bars]
        ranked = []
        for symbol, current_price in current.items():
            point = _asof_funding(funding_series.get(symbol, []), timestamp, max_age_multiplier)
            forward = forward_return_pct(current_price, future.get(symbol))
            if point is not None and forward is not None:
                ranked.append((symbol, point[1], forward, point[2]))
        if len(ranked) < top_k * 2:
            continue
        ranked.sort(key=lambda row: (row[1], row[0]))
        dispersion_bps = (ranked[-1][1] - ranked[0][1]) * 10_000.0
        if dispersion_bps < min_dispersion_bps:
            continue
        low = ranked[:top_k]
        high = ranked[-top_k:]
        low_return = statistics.mean(row[2] for row in low)
        high_return = statistics.mean(row[2] for row in high)
        observations.append({
            "ts_ms": timestamp,
            "forward_ts_ms": forward_timestamp,
            "funding_dispersion_bps": dispersion_bps,
            "low_funding_symbols": [row[0] for row in low],
            "high_funding_symbols": [row[0] for row in reversed(high)],
            "low_funding_rates_pct": {row[0]: row[1] * 100.0 for row in low},
            "high_funding_rates_pct": {row[0]: row[1] * 100.0 for row in reversed(high)},
            "low_funding_forward_return_pct": low_return,
            "high_funding_forward_return_pct": high_return,
            "low_minus_high_forward_return_pct": low_return - high_return,
        })
    return observations


def summarize(observations, min_observations, min_edge_bps, paper_cost_bps=0.0):
    edges = [row["low_minus_high_forward_return_pct"] * 100.0 for row in observations]
    adjusted = [edge - paper_cost_bps for edge in edges]
    candidate = (len(observations) >= min_observations and adjusted
                 and statistics.mean(adjusted) >= min_edge_bps)
    return {
        "observations": len(observations),
        "mean_low_minus_high_edge_bps": statistics.mean(edges) if edges else None,
        "median_low_minus_high_edge_bps": statistics.median(edges) if edges else None,
        "positive_edge_hit_rate": (sum(edge > 0 for edge in edges) / len(edges)) if edges else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_edge_bps": statistics.mean(adjusted) if adjusted else None,
        "cost_adjusted_positive_edge_hit_rate": (
            sum(edge > 0 for edge in adjusted) / len(adjusted) if adjusted else None
        ),
        "verdict": "cross_sectional_funding_candidate" if candidate else "observe_only",
        "evidence": [
            "funding_dispersion_and_exact_price_intersection" if observations
            else "no_qualifying_cross_sectional_funding_windows",
            "cost_adjusted_edge_above_threshold" if candidate
            else "edge_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default="BTCUSDT,ETHUSDT,SOLUSDT")
    parser.add_argument("--funding-exchange", default="binance")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--funding-limit", type=int, default=500)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--top-k", type=int, default=1)
    parser.add_argument("--min-dispersion-bps", type=float, default=1.0)
    parser.add_argument("--max-age-multiplier", type=float, default=1.5)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    symbols = sorted({item.strip().upper() for item in options.symbols.split(",") if item.strip()})
    if (len(symbols) < 2 or options.days <= 0 or options.funding_limit <= 0
            or options.price_limit <= 0 or options.horizon_bars <= 0 or options.top_k <= 0
            or options.top_k * 2 > len(symbols) or options.min_dispersion_bps < 0
            or options.max_age_multiplier <= 0 or options.paper_cost_bps < 0
            or options.min_edge_bps < 0 or options.min_observations <= 0 or options.timeout <= 0):
        parser.error("invalid symbols, limits, windows, dispersion, cost or observation arguments")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(options.days * 86_400_000)
    funding_series = {}
    price_series = {}
    funding_coverage = {}
    price_coverage = {}
    errors = []
    for symbol in symbols:
        funding_payload = fetch(options.base_url, "/v1/history/candles", {
            "exchange": options.funding_exchange, "symbol": symbol,
            "candle_type": "funding_rate", "start_ms": start_ms, "end_ms": now_ms,
            "limit": min(options.funding_limit, 500),
        }, options.timeout)
        price_payload = fetch(options.base_url, "/v1/history/candles", {
            "exchange": options.price_exchange, "market": options.market, "symbol": symbol,
            "interval": options.interval, "start_ms": start_ms, "end_ms": now_ms,
            "limit": min(options.price_limit, 1000),
        }, options.timeout)
        funding_series[symbol] = funding_points(funding_payload)
        price_series[symbol] = price_points(price_payload)
        funding_coverage[symbol] = funding_payload.get("coverage_detail")
        price_coverage[symbol] = price_payload.get("coverage_detail")
        if funding_payload.get("error"):
            errors.append({"symbol": symbol, "source": "funding", "error": funding_payload["error"]})
        if price_payload.get("error"):
            errors.append({"symbol": symbol, "source": "price", "error": price_payload["error"]})

    observations = cross_section_observations(
        funding_series, price_series, options.horizon_bars, options.top_k,
        options.min_dispersion_bps, options.max_age_multiplier,
    )
    summary = summarize(observations, options.min_observations,
                        options.min_edge_bps, options.paper_cost_bps)
    evidence = list(summary.pop("evidence"))
    for symbol, detail in {**funding_coverage, **price_coverage}.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{symbol}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_funding_cross_section_replay",
        "symbols": symbols,
        "venues": {"funding": options.funding_exchange, "price": options.price_exchange},
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": options.days},
        "filters": {
            "horizon_bars": options.horizon_bars,
            "top_k": options.top_k,
            "min_dispersion_bps": options.min_dispersion_bps,
            "max_age_multiplier": options.max_age_multiplier,
            "paper_cost_bps": options.paper_cost_bps,
            "min_edge_bps": options.min_edge_bps,
            "min_observations": options.min_observations,
        },
        "source_counts": {
            "funding_points": {symbol: len(points) for symbol, points in funding_series.items()},
            "price_points": {symbol: len(points) for symbol, points in price_series.items()},
            "aligned_price_points": len(aligned_price_points(price_series)),
        },
        "observations": observations,
        "summary": summary,
        "coverage": {"funding": funding_coverage, "price": price_coverage},
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "funding is a crowding proxy, not a long/short ownership ledger",
            "as-of funding values are bounded by point-in-time interval freshness",
            "exact price intersections can reduce sample size and do not model fills",
            "paper cost is a user-supplied hurdle, not venue fees, funding cash flow, borrow or slippage",
            "no allocation, hedge, leverage, liquidation or order execution is modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
