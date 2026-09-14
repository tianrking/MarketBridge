#!/usr/bin/env python3
"""Replay persistence and contraction of a same-asset cross-venue price gap.

The falsifiable hypothesis is narrow: after the same symbol's synchronized
prices on two venues move unusually far apart from their trailing log-gap mean,
does the gap contract over a fixed horizon? This is a market-fragmentation
diagnostic, not an arbitrage order, transfer plan or executable PnL estimate.
"""

import argparse
import json
import math
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


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def aligned_points(left, right):
    left_map = dict(left)
    right_map = dict(right)
    return [(timestamp, left_map[timestamp], right_map[timestamp])
            for timestamp in sorted(set(left_map) & set(right_map))]


def gap_series(points):
    return [(timestamp, math.log(left / right) * 10_000.0)
            for timestamp, left, right in points if left > 0 and right > 0]


def gap_observations(points, lookback_bars, horizon_bars, entry_z):
    gaps = gap_series(points)
    observations = []
    for index in range(lookback_bars, len(gaps) - horizon_bars):
        history = [value for _, value in gaps[index - lookback_bars:index]]
        deviation = statistics.pstdev(history)
        if deviation <= 0:
            continue
        timestamp, current = gaps[index]
        mean = statistics.mean(history)
        z_score = (current - mean) / deviation
        if abs(z_score) < entry_z:
            continue
        future_timestamp, future = gaps[index + horizon_bars]
        current_distance = abs(current - mean)
        future_distance = abs(future - mean)
        observations.append({
            "ts_ms": timestamp,
            "forward_ts_ms": future_timestamp,
            "gap_bps": current,
            "future_gap_bps": future,
            "frozen_mean_bps": mean,
            "trailing_std_bps": deviation,
            "z_score": z_score,
            "future_z_score_against_frozen_mean": (future - mean) / deviation,
            "contraction_bps": current_distance - future_distance,
            "contracted": future_distance < current_distance,
        })
    return observations


def summarize(observations, min_observations, min_contraction_bps, paper_cost_bps=0.0):
    gross = [row["contraction_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    candidate = (len(observations) >= min_observations and adjusted
                 and statistics.mean(adjusted) >= min_contraction_bps)
    return {
        "signals": len(observations),
        "contracted_signals": sum(row["contracted"] for row in observations),
        "contraction_rate": (sum(row["contracted"] for row in observations) / len(observations)
                             if observations else None),
        "mean_contraction_bps": statistics.mean(gross) if gross else None,
        "median_contraction_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_contraction_bps": statistics.mean(adjusted) if adjusted else None,
        "verdict": "cross_venue_gap_contraction_candidate" if candidate else "observe_only",
        "evidence": [
            "synchronized_cross_venue_gap_signals_available" if observations else "no_gap_signal",
            "cost_adjusted_gap_contraction_above_threshold" if candidate
            else "contraction_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange-a", default="binance")
    parser.add_argument("--exchange-b", default="okx")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="spot")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--lookback-bars", type=int, default=24)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--entry-z", type=float, default=2.0)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-contraction-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.exchange_a.lower() == args.exchange_b.lower() or args.days <= 0 or args.limit <= 0
            or args.lookback_bars <= 1 or args.horizon_bars <= 0 or args.entry_z < 0
            or args.paper_cost_bps < 0 or args.min_contraction_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid venues, windows, z-score, cost or observation arguments")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    payloads = {}
    for exchange in (args.exchange_a.lower(), args.exchange_b.lower()):
        payloads[exchange] = fetch(args.base_url, "/v1/history/candles", {
            "exchange": exchange, "market": args.market, "symbol": args.symbol,
            "interval": args.interval, "start_ms": start_ms, "end_ms": now_ms,
            "limit": min(args.limit, 1000),
        }, args.timeout)
    exchange_a, exchange_b = args.exchange_a.lower(), args.exchange_b.lower()
    points = aligned_points(price_points(payloads[exchange_a]), price_points(payloads[exchange_b]))
    observations = gap_observations(points, args.lookback_bars, args.horizon_bars, args.entry_z)
    summary = summarize(observations, args.min_observations,
                        args.min_contraction_bps, args.paper_cost_bps)
    evidence = list(summary.pop("evidence"))
    coverage = {exchange: payloads[exchange].get("coverage_detail")
                for exchange in (exchange_a, exchange_b)}
    for exchange, detail in coverage.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{exchange}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_cross_venue_price_gap_replay",
        "market": {"symbol": args.symbol, "market": args.market, "interval": args.interval},
        "venues": [exchange_a, exchange_b],
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {
            "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
            "entry_z": args.entry_z, "paper_cost_bps": args.paper_cost_bps,
            "min_contraction_bps": args.min_contraction_bps,
            "min_observations": args.min_observations,
        },
        "source_counts": {exchange_a: len(price_points(payloads[exchange_a])),
                          exchange_b: len(price_points(payloads[exchange_b])),
                          "aligned_points": len(points)},
        "observations": observations,
        "summary": summary,
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": [
            {"exchange": exchange, "error": payloads[exchange].get("error")}
            for exchange in (exchange_a, exchange_b) if payloads[exchange].get("error")
        ],
        "limitations": [
            "public candles can be bounded, asynchronous or stale across venues",
            "gap contraction is not a simultaneous bid/ask executable spread",
            "paper cost is a caller hurdle, not fees, transfer cost, inventory, latency or slippage",
            "no balance, transfer, hedge, order, sizing or live execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
