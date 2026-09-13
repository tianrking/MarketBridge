#!/usr/bin/env python3
"""Replay a funding-extreme plus open-interest crowding hypothesis.

Public crypto dashboards often place funding, open interest and liquidations
next to each other.  This example turns that intuition into a falsifiable
read-only test: when funding is extreme and OI is rising, do the next few
price bars move against the crowded side?  Missing OI, funding schedules or
forward candles remain missing; they are never filled with zero.
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
        rate = numeric(row.get("close"))
        if isinstance(timestamp, int) and rate is not None:
            points.append((timestamp, rate, intervals.get(timestamp)))
    return sorted(set(points))


def oi_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        value = numeric(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
    return sorted(set(points))


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = numeric(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def oi_change_at(timestamp, points):
    previous = [point for point in points if point[0] <= timestamp]
    if len(previous) < 2 or previous[-2][1] <= 0:
        return None
    return (previous[-1][1] - previous[-2][1]) / previous[-2][1] * 100.0


def forward_return(timestamp, points, horizon_bars):
    after = [point for point in points if point[0] >= timestamp]
    if len(after) <= horizon_bars or after[0][1] <= 0:
        return None
    return (after[horizon_bars][1] - after[0][1]) / after[0][1] * 100.0


def classify_state(funding_pct, oi_change_pct, min_funding_pct, min_oi_change_pct):
    if oi_change_pct is None:
        return "missing_oi"
    if funding_pct >= min_funding_pct and oi_change_pct >= min_oi_change_pct:
        return "long_crowded"
    if funding_pct <= -min_funding_pct and oi_change_pct >= min_oi_change_pct:
        return "short_crowded"
    return "other"


def summarize(rows):
    summary = {}
    for state in ("long_crowded", "short_crowded", "other", "missing_oi"):
        selected = [row for row in rows if row["state"] == state]
        returns = [row["forward_return_pct"] for row in selected if row["forward_return_pct"] is not None]
        expected = None
        if state in ("long_crowded", "short_crowded") and returns:
            sign = -1.0 if state == "long_crowded" else 1.0
            expected = sum(sign * value >= 0.0 for value in returns) / len(returns)
        summary[state] = {
            "observations": len(selected),
            "forward_observations": len(returns),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "median_forward_return_pct": statistics.median(returns) if returns else None,
            "expected_direction_hit_rate": expected,
        }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--funding-exchange", default="binance")
    parser.add_argument("--oi-exchange", default="binance")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--min-funding-pct", type=float, default=0.01)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.10)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (
        options.days <= 0
        or options.limit <= 0
        or options.min_funding_pct < 0
        or options.min_oi_change_pct < 0
        or options.horizon_bars <= 0
        or options.min_observations <= 0
    ):
        parser.error("days, limit, thresholds, horizon-bars and min-observations must be valid")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(options.days * 86_400_000)
    common = {
        "symbol": options.symbol,
        "start_ms": start_ms,
        "end_ms": now_ms,
        "limit": min(options.limit, 500),
    }
    funding_payload = fetch(options.base_url, "/v1/history/candles", {
        **common,
        "exchange": options.funding_exchange,
        "candle_type": "funding_rate",
    }, options.timeout)
    oi_payload = fetch(options.base_url, "/v1/history/open-interest", {
        **common,
        "exchange": options.oi_exchange,
        "interval": "5m",
    }, options.timeout)
    price_payload = fetch(options.base_url, "/v1/history/candles", {
        **common,
        "exchange": options.price_exchange,
        "candle_type": "perp",
        "interval": "5m",
    }, options.timeout)
    funding = funding_points(funding_payload)
    oi = oi_points(oi_payload)
    prices = candle_points(price_payload)
    rows = []
    for timestamp, rate, interval_ms in funding:
        funding_pct = rate * 100.0
        oi_change_pct = oi_change_at(timestamp, oi)
        state = classify_state(
            funding_pct,
            oi_change_pct,
            options.min_funding_pct,
            options.min_oi_change_pct,
        )
        rows.append({
            "ts_ms": timestamp,
            "funding_rate": rate,
            "funding_pct": funding_pct,
            "funding_interval_ms": interval_ms,
            "oi_change_pct": oi_change_pct,
            "state": state,
            "forward_return_pct": forward_return(timestamp, prices, options.horizon_bars),
        })
    summary = summarize(rows)
    qualifying = [
        row for row in rows
        if row["state"] in ("long_crowded", "short_crowded")
        and row["forward_return_pct"] is not None
    ]
    errors = [
        {"source": "funding", "error": funding_payload["error"]}
        for _ in [0]
        if funding_payload.get("error")
    ] + [
        {"source": "open_interest", "error": oi_payload["error"]}
        for _ in [0]
        if oi_payload.get("error")
    ] + [
        {"source": "price", "error": price_payload["error"]}
        for _ in [0]
        if price_payload.get("error")
    ]
    evidence = []
    if funding:
        evidence.append("funding_history_available")
    if oi:
        evidence.append("open_interest_history_available")
    if prices:
        evidence.append("forward_price_history_available")
    if not qualifying:
        evidence.append("no_crowded_events_with_forward_returns")
    print(json.dumps({
        "strategy": "crypto_funding_oi_replay",
        "symbol": options.symbol,
        "venues": {
            "funding": options.funding_exchange,
            "open_interest": options.oi_exchange,
            "price": options.price_exchange,
        },
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": options.days},
        "filters": {
            "min_funding_pct": options.min_funding_pct,
            "min_oi_change_pct": options.min_oi_change_pct,
            "horizon_bars": options.horizon_bars,
            "min_observations": options.min_observations,
        },
        "source_counts": {"funding": len(funding), "open_interest": len(oi), "price_bars": len(prices)},
        "observations": rows,
        "summary": summary,
        "verdict": "crowding replay candidate" if len(qualifying) >= options.min_observations else "observe only",
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "funding and OI are from public aggregate histories and may be different venues",
            "forward return is a price observation, not a hedge or fill result",
            "fees, borrow, margin, transfer latency, mark/index divergence, slippage and liquidation are excluded",
            "missing schedules or timestamps are retained as missing evidence",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
