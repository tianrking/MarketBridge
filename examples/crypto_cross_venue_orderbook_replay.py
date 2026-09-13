#!/usr/bin/env python3
"""Replay persistence of synchronized cross-venue book-edge candidates."""

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


def longest_run(states, target):
    best = current = 0
    for state in states:
        current = current + 1 if state == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run, min_net_edge_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    edges = []
    qualifying = []
    states = []
    for record in ordered:
        observation = record.get("observation") or {}
        best = observation.get("best_opportunity") or {}
        edge = best.get("net_edge_bps")
        state = observation.get("state", "observe_only_no_synchronized_depth")
        qualifies = isinstance(edge, (int, float)) and edge >= min_net_edge_bps
        states.append("qualifying_cross_venue_book_edge" if qualifies else state)
        if isinstance(edge, (int, float)):
            edges.append(float(edge))
        if qualifies:
            qualifying.append(float(edge))
    run = longest_run(states, "qualifying_cross_venue_book_edge")
    return {
        "snapshots": len(ordered),
        "edge_observations": len(edges),
        "qualifying_observations": len(qualifying),
        "mean_net_edge_bps": statistics.mean(edges) if edges else None,
        "mean_qualifying_net_edge_bps": statistics.mean(qualifying) if qualifying else None,
        "longest_qualifying_run": run,
        "verdict": "persistent_cross_venue_book_edge_candidate"
        if run >= min_run else "observe_only_no_persistent_book_edge",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_cross_venue_orderbook_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"min_run": args.min_run, "min_net_edge_bps": args.min_net_edge_bps},
        "summary": summarize_records(records, args.min_run, args.min_net_edge_bps),
        "limitations": [
            "consecutive snapshots do not prove simultaneous fills or inventory neutrality",
            "paper costs exclude transfer latency, withdrawal limits, borrow and venue failure",
            "a book edge is a research observation, not an arbitrage or execution instruction",
            "no wallet, order or live execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
