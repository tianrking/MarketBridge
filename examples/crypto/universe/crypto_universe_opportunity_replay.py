#!/usr/bin/env python3
"""Replay persistence of bounded cross-sectional universe candidates."""

import argparse
import json
from pathlib import Path


def load_records(path):
    records = []
    invalid_lines = 0
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


def longest_run(values, target):
    best = current = 0
    for value in values:
        current = current + 1 if value == target else 0
        best = max(best, current)
    return best


def candidate_symbols(record, top_k):
    rows = (record.get("observation") or {}).get("candidates", [])
    symbols = [str(row.get("symbol", "")).upper() for row in rows if row.get("symbol")]
    return tuple(symbols[:top_k])


def summarize_records(records, top_k, min_run):
    top_sets = [candidate_symbols(record, top_k) for record in records]
    nonempty = [symbols for symbols in top_sets if symbols]
    symbol_counts = {}
    for symbols in top_sets:
        for symbol in symbols:
            symbol_counts[symbol] = symbol_counts.get(symbol, 0) + 1
    states = ["candidate_set_available" if symbols else "no_candidates" for symbols in top_sets]
    persistent_symbols = {
        symbol: count / len(records)
        for symbol, count in symbol_counts.items()
        if records and count / len(records) >= 0.5
    }
    return {
        "snapshots": len(records),
        "candidate_set_snapshots": len(nonempty),
        "candidate_set_fraction": len(nonempty) / len(records) if records else None,
        "top_k": top_k,
        "symbol_snapshot_fraction": {
            symbol: count / len(records) for symbol, count in sorted(symbol_counts.items())
        } if records else {},
        "persistent_symbols": persistent_symbols,
        "longest_candidate_set_run": longest_run(states, "candidate_set_available"),
        "state_counts": {state: states.count(state) for state in sorted(set(states))},
        "verdict": (
            "persistent_universe_candidate_set"
            if len(nonempty) >= min_run and longest_run(states, "candidate_set_available") >= min_run
            else "observe_only_no_persistent_candidate_set"
        ),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--top-k", type=int, default=3)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.top_k <= 0 or options.min_run <= 0:
        parser.error("top-k and min-run must be positive")
    records, invalid_lines = load_records(options.input)
    print(json.dumps({
        "strategy": "crypto_universe_opportunity_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "filters": {"top_k": options.top_k, "min_run": options.min_run},
        "summary": summarize_records(records, options.top_k, options.min_run),
        "limitations": [
            "candidate persistence is descriptive and does not imply allocation or returns",
            "funding is a current snapshot and rankings can change with provider coverage",
            "missing joins and empty candidate sets remain explicit",
            "no fees, borrow, basis, liquidity impact, capacity, hedge or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
