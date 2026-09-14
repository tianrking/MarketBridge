#!/usr/bin/env python3
"""Replay persistence of recorded put/call open-interest states."""

import argparse
import json
from pathlib import Path


def load_records(path):
    records, invalid_lines = [], 0
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                invalid_lines += 1
                continue
            if isinstance(row, dict) and isinstance(row.get("observation"), dict):
                records.append(row)
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
    states = [(row.get("observation") or {}).get("state", "observe_only_missing_put_call_oi")
              for row in ordered]
    counts = {state: states.count(state) for state in sorted(set(states))}
    defensive_run = longest_run(states, "defensive_put_oi")
    call_run = longest_run(states, "call_dominant_oi")
    return {
        "snapshots": len(ordered),
        "state_counts": counts,
        "defensive_put_oi_longest_run": defensive_run,
        "call_dominant_oi_longest_run": call_run,
        "verdict": "persistent_defensive_put_oi_candidate" if defensive_run >= min_run
        else "persistent_call_dominant_oi_candidate" if call_run >= min_run
        else "observe_only_no_persistent_put_call_oi_state",
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
    print(json.dumps({"strategy": "crypto_options_put_call_oi_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"min_run": args.min_run},
                      "summary": summarize_records(records, args.min_run),
                      "limitations": [
                          "open-interest state is provider snapshot context with expiry roll",
                          "persistence does not establish direction, dealer positioning or option PnL",
                          "no hedge, margin, cost, allocation or execution model is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
