#!/usr/bin/env python3
"""Replay persistence of recorded aggregate derivatives sentiment states."""

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


def longest_run(states, target):
    best = current = 0
    for state in states:
        current = current + 1 if state == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    states = [(row.get("observation") or {}).get("state", "observe_only_missing_positioning_metrics")
              for row in ordered]
    crowding = {state: states.count(state) for state in sorted(set(states))}
    long_run = longest_run(states, "long_crowding_context")
    short_run = longest_run(states, "short_crowding_context")
    persistent = max(long_run, short_run) >= min_run
    return {
        "snapshots": len(ordered),
        "state_counts": crowding,
        "long_crowding_longest_run": long_run,
        "short_crowding_longest_run": short_run,
        "verdict": "persistent_derivatives_crowding_candidate" if persistent
        else "observe_only_no_persistent_crowding",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_derivatives_sentiment_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"min_run": args.min_run},
        "summary": summarize_records(records, args.min_run),
        "limitations": [
            "aggregate sentiment states are provider snapshots, not position ownership",
            "persistence is descriptive and does not imply price direction or funding income",
            "no allocation, hedge, wallet or execution model is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
