#!/usr/bin/env python3
"""Replay a persistent extreme-funding regime hypothesis.

The hypothesis is deliberately narrow and falsifiable: after several
consecutive funding observations remain unusually positive (crowded longs) or
negative (crowded shorts), does the next price window move in the opposite
direction?  This is a read-only research replay; it does not model a hedge,
funding cash flow, fills, or orders.
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


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = numeric(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def _side(rate, threshold):
    if rate >= threshold:
        return "positive"
    if rate <= -threshold:
        return "negative"
    return None


def funding_runs(points, threshold, min_run):
    """Return contiguous extreme runs; neutral points terminate a run."""
    runs = []
    current = []
    current_side = None
    for point in points:
        side = _side(point[1], threshold)
        if side is None:
            if len(current) >= min_run:
                runs.append(_run_record(current_side, current))
            current, current_side = [], None
            continue
        interval_ms = current[-1][2] if current else None
        gap = point[0] - current[-1][0] if current else None
        contiguous = not current or current_side == side and (
            interval_ms is None or gap <= int(interval_ms * 1.5)
        )
        if not contiguous:
            if len(current) >= min_run:
                runs.append(_run_record(current_side, current))
            current = []
        current_side = side
        current.append(point)
    if len(current) >= min_run:
        runs.append(_run_record(current_side, current))
    return runs


def _run_record(side, points):
    rates = [point[1] for point in points]
    return {
        "side": side,
        "start_ts_ms": points[0][0],
        "end_ts_ms": points[-1][0],
        "observations": len(points),
        "mean_funding_pct": statistics.mean(rates) * 100.0,
        "min_funding_pct": min(rates) * 100.0,
        "max_funding_pct": max(rates) * 100.0,
        "funding_interval_ms": points[-1][2],
    }


def forward_return(timestamp, points, horizon_bars):
    after = [point for point in points if point[0] >= timestamp]
    if len(after) <= horizon_bars or after[0][1] <= 0:
        return None
    return (after[horizon_bars][1] - after[0][1]) / after[0][1] * 100.0


def summarize(runs):
    summary = {}
    for side in ("positive", "negative"):
        selected = [run for run in runs if run["side"] == side]
        returns = [run["forward_return_pct"] for run in selected if run["forward_return_pct"] is not None]
        expected = -1.0 if side == "positive" else 1.0
        summary[side] = {
            "runs": len(selected),
            "forward_observations": len(returns),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "median_forward_return_pct": statistics.median(returns) if returns else None,
            "expected_direction_hit_rate": (
                sum(expected * value >= 0.0 for value in returns) / len(returns)
                if returns else None
            ),
        }
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--funding-exchange", default="binance")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--funding-limit", type=int, default=500)
    parser.add_argument("--price-limit", type=int, default=500)
    parser.add_argument("--min-funding-pct", type=float, default=0.01)
    parser.add_argument("--min-run", type=int, default=3)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (
        options.days <= 0 or options.funding_limit <= 0 or options.price_limit <= 0
        or options.min_funding_pct < 0 or options.min_run <= 0
        or options.horizon_bars <= 0 or options.min_observations <= 0
    ):
        parser.error("days, limits, threshold, min-run, horizon-bars and min-observations must be valid")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(options.days * 86_400_000)
    common = {"symbol": options.symbol, "start_ms": start_ms, "end_ms": now_ms}
    funding_payload = fetch(options.base_url, "/v1/history/candles", {
        **common, "exchange": options.funding_exchange,
        "candle_type": "funding_rate", "limit": min(options.funding_limit, 500),
    }, options.timeout)
    price_payload = fetch(options.base_url, "/v1/history/candles", {
        **common, "exchange": options.price_exchange,
        "candle_type": "perp", "interval": "5m", "limit": min(options.price_limit, 500),
    }, options.timeout)
    funding = funding_points(funding_payload)
    prices = price_points(price_payload)
    runs = funding_runs(funding, options.min_funding_pct / 100.0, options.min_run)
    for run in runs:
        run["forward_return_pct"] = forward_return(run["end_ts_ms"], prices, options.horizon_bars)
    summary = summarize(runs)
    qualifying = [run for run in runs if run["forward_return_pct"] is not None]
    errors = []
    for source, payload in (("funding", funding_payload), ("price", price_payload)):
        if payload.get("error"):
            errors.append({"source": source, "error": payload["error"]})
    evidence = []
    if funding:
        evidence.append("funding_history_available")
    if prices:
        evidence.append("forward_price_history_available")
    if not qualifying:
        evidence.append("no_persistent_extreme_runs_with_forward_returns")
    print(json.dumps({
        "strategy": "crypto_funding_regime_replay",
        "symbol": options.symbol,
        "venues": {"funding": options.funding_exchange, "price": options.price_exchange},
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": options.days},
        "filters": {"min_funding_pct": options.min_funding_pct, "min_run": options.min_run,
                    "horizon_bars": options.horizon_bars, "min_observations": options.min_observations},
        "source_counts": {"funding": len(funding), "price_bars": len(prices)},
        "runs": runs,
        "summary": summary,
        "verdict": "persistent funding replay candidate" if len(qualifying) >= options.min_observations else "observe only",
        "evidence": evidence,
        "upstream_errors": errors,
        "limitations": [
            "positive/negative funding is treated as a crowding proxy, not a position ledger",
            "forward return is a price observation, not a hedge or fill result",
            "funding cash flow, fees, borrow, margin, slippage and liquidation are excluded",
            "unknown funding schedules break contiguity only when a known interval is violated",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
