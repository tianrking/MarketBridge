#!/usr/bin/env python3
"""Replay BTC response after frozen cross-venue order-book edge states."""

import argparse
import json
import statistics
from pathlib import Path


def load_records(path):
    records, invalid_lines = [], 0
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                invalid_lines += 1
                continue
            if isinstance(record, dict) and isinstance(record.get("observation"), dict):
                records.append(record)
            else:
                invalid_lines += 1
    return records, invalid_lines


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def edge_state(observation, min_net_edge_bps):
    best = observation.get("best_opportunity") or {}
    edge = number(best.get("net_edge_bps"))
    if edge is None:
        return "missing_book_edge"
    return "qualifying_book_edge" if edge >= min_net_edge_bps else "edge_below_paper_hurdle"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize_records(records, horizon_records, min_net_edge_bps, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        current = number((((record.get("observation") or {}).get("price") or {}).get("price")))
        future = number((((ordered[future_index].get("observation") or {}).get("price") or {}).get("price")))
        if current is None or future is None or current <= 0 or future <= 0:
            continue
        observation = record.get("observation") or {}
        best = observation.get("best_opportunity") or {}
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": edge_state(observation, min_net_edge_bps),
            "net_edge_bps": best.get("net_edge_bps"),
            "buy_exchange": best.get("buy_exchange"),
            "sell_exchange": best.get("sell_exchange"),
            "forward_return_pct": (future / current - 1.0) * 100.0,
        })
    states = ("qualifying_book_edge", "edge_below_paper_hurdle", "missing_book_edge")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    return {
        "by_state": by_state, "aligned_forward_windows": len(observations),
        "qualifying_aligned_windows": by_state["qualifying_book_edge"]["observations"],
        "verdict": ("cross_venue_book_response_reported"
                    if by_state["qualifying_book_edge"]["observations"] >= min_observations
                    else "observe_only_insufficient_aligned_book_edges"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_net_edge_bps < 0 or args.min_observations <= 0:
        parser.error("horizon and minimum observations must be positive; edge cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_cross_venue_orderbook_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_net_edge_bps": args.min_net_edge_bps,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_net_edge_bps,
                                      args.min_observations),
        "limitations": [
            "a qualifying book edge is a snapshot observation, not a simultaneous fill or arbitrage PnL",
            "future BTC return is descriptive and does not model inventory, transfers, queue or settlement",
            "paper cost is a sensitivity hurdle rather than venue fees, borrow or slippage",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
