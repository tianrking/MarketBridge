#!/usr/bin/env python3
"""Replay persistence of near-expiry max-pain proxy states."""

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
    states = [((row.get("observation") or {}).get("target_expiry") or {}).get(
        "state", "observe_only_insufficient_max_pain_inputs") for row in ordered]
    counts = {state: states.count(state) for state in sorted(set(states))}
    near_run = longest_run(states, "near_expiry_near_max_pain")
    far_run = longest_run(states, "near_expiry_far_from_max_pain")
    return {
        "snapshots": len(ordered),
        "state_counts": counts,
        "near_expiry_near_max_pain_longest_run": near_run,
        "near_expiry_far_from_max_pain_longest_run": far_run,
        "verdict": "persistent_near_expiry_max_pain_candidate" if near_run >= min_run
        else "persistent_near_expiry_dislocation_candidate" if far_run >= min_run
        else "observe_only_no_persistent_max_pain_state",
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
    print(json.dumps({"strategy": "crypto_options_max_pain_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"min_run": args.min_run},
                      "summary": summarize_records(records, args.min_run),
                      "limitations": [
                          "max-pain proxy is provider-unit intrinsic context, not option PnL",
                          "expiry roll and Deribit TWAP settlement remain explicit gaps",
                          "persistence does not prove price pinning, intent or execution",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
