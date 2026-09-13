#!/usr/bin/env python3
"""Replay a bounded two-asset crypto spread mean-reversion hypothesis.

The falsifiable hypothesis is narrow: after the log-price spread of two
caller-selected assets moves beyond a trailing z-score threshold, does its
absolute deviation from the *frozen* trailing mean shrink over a fixed future
horizon? This is a relative-price diagnostic, not a pairs order, hedge, PnL or
cointegration certification.
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


def candle_points(payload):
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


def spread_series(points, hedge_ratio):
    if not math.isfinite(hedge_ratio):
        return []
    return [(timestamp, math.log(left) - hedge_ratio * math.log(right))
            for timestamp, left, right in points if left > 0 and right > 0]


def spread_observations(points, lookback_bars, horizon_bars, entry_z, hedge_ratio):
    spreads = spread_series(points, hedge_ratio)
    observations = []
    for index in range(lookback_bars, len(spreads) - horizon_bars):
        history = [value for _, value in spreads[index - lookback_bars:index]]
        deviation = statistics.pstdev(history)
        if deviation <= 0:
            continue
        timestamp, current = spreads[index]
        mean = statistics.mean(history)
        z_score = (current - mean) / deviation
        if abs(z_score) < entry_z:
            continue
        future_timestamp, future = spreads[index + horizon_bars]
        current_distance = abs(current - mean)
        future_distance = abs(future - mean)
        observations.append({
            "ts_ms": timestamp,
            "forward_ts_ms": future_timestamp,
            "spread": current,
            "future_spread": future,
            "frozen_mean": mean,
            "trailing_std": deviation,
            "z_score": z_score,
            "future_z_score_against_frozen_mean": (future - mean) / deviation,
            "current_distance_bps": current_distance * 10_000.0,
            "future_distance_bps": future_distance * 10_000.0,
            "convergence_bps": (current_distance - future_distance) * 10_000.0,
            "converged": future_distance < current_distance,
        })
    return observations


def summarize(observations, min_observations, min_convergence_bps, paper_cost_bps=0.0):
    gross = [row["convergence_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    candidate = (len(observations) >= min_observations and adjusted
                 and statistics.mean(adjusted) >= min_convergence_bps)
    return {
        "signals": len(observations),
        "converged_signals": sum(row["converged"] for row in observations),
        "convergence_rate": (
            sum(row["converged"] for row in observations) / len(observations)
            if observations else None
        ),
        "mean_convergence_bps": statistics.mean(gross) if gross else None,
        "median_convergence_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_convergence_bps": statistics.mean(adjusted) if adjusted else None,
        "verdict": "spread_convergence_candidate" if candidate else "observe_only",
        "evidence": [
            "trailing_spread_zscore_signals_available" if observations else "no_spread_signal",
            "cost_adjusted_convergence_above_threshold" if candidate
            else "convergence_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol-a", default="BTCUSDT")
    parser.add_argument("--symbol-b", default="ETHUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--limit", type=int, default=500)
    parser.add_argument("--hedge-ratio", type=float, default=1.0)
    parser.add_argument("--lookback-bars", type=int, default=24)
    parser.add_argument("--horizon-bars", type=int, default=6)
    parser.add_argument("--entry-z", type=float, default=2.0)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-convergence-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.symbol_a.upper() == args.symbol_b.upper() or args.days <= 0 or args.limit <= 0
            or args.hedge_ratio <= 0 or args.lookback_bars <= 1 or args.horizon_bars <= 0
            or args.entry_z < 0 or args.paper_cost_bps < 0 or args.min_convergence_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid pair, windows, hedge ratio, z-score, cost or observation arguments")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    payloads = {}
    for symbol in (args.symbol_a.upper(), args.symbol_b.upper()):
        payloads[symbol] = fetch(args.base_url, "/v1/history/candles", {
            "exchange": args.exchange, "market": args.market, "symbol": symbol,
            "interval": args.interval, "start_ms": start_ms, "end_ms": now_ms,
            "limit": min(args.limit, 1000),
        }, args.timeout)
    left, right = args.symbol_a.upper(), args.symbol_b.upper()
    points = aligned_points(candle_points(payloads[left]), candle_points(payloads[right]))
    observations = spread_observations(points, args.lookback_bars, args.horizon_bars,
                                       args.entry_z, args.hedge_ratio)
    summary = summarize(observations, args.min_observations,
                        args.min_convergence_bps, args.paper_cost_bps)
    evidence = list(summary.pop("evidence"))
    coverage = {symbol: payloads[symbol].get("coverage_detail") for symbol in payloads}
    for symbol, detail in coverage.items():
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{symbol}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_pairs_mean_reversion_replay",
        "pair": {"symbol_a": left, "symbol_b": right, "hedge_ratio": args.hedge_ratio},
        "venue": {"exchange": args.exchange, "market": args.market, "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {
            "lookback_bars": args.lookback_bars,
            "horizon_bars": args.horizon_bars,
            "entry_z": args.entry_z,
            "paper_cost_bps": args.paper_cost_bps,
            "min_convergence_bps": args.min_convergence_bps,
            "min_observations": args.min_observations,
        },
        "source_counts": {
            "aligned_price_points": len(points),
            left: len(candle_points(payloads[left])),
            right: len(candle_points(payloads[right])),
        },
        "observations": observations,
        "summary": summary,
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": [
            {"symbol": symbol, "error": payload.get("error")}
            for symbol, payload in payloads.items() if payload.get("error")
        ],
        "limitations": [
            "a fixed hedge ratio and rolling spread are not a cointegration test",
            "the trailing mean and standard deviation are frozen at each signal, with no future data leakage",
            "convergence is relative-price movement, not paired fills, PnL or market neutrality",
            "paper cost is a sensitivity hurdle, not venue fees, borrow, funding, margin or slippage",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
