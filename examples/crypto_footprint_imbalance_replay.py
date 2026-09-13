#!/usr/bin/env python3
"""Replay persistence of footprint bid/ask pressure states."""

import argparse
import json
from pathlib import Path


def load_records(path):
    rows, invalid_lines = [], 0
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
                rows.append(row)
            else:
                invalid_lines += 1
    return rows, invalid_lines


def longest_run(states, targets):
    best = current = 0
    for state in states:
        current = current + 1 if state in targets else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    states = [((row.get("observation") or {}).get("state", "observe_only_missing_footprint"))
              for row in ordered]
    pressure = {"footprint_bid_pressure", "footprint_ask_pressure"}
    return {"snapshots": len(ordered), "pressure_snapshots": sum(state in pressure for state in states),
            "bid_pressure_snapshots": states.count("footprint_bid_pressure"),
            "ask_pressure_snapshots": states.count("footprint_ask_pressure"),
            "longest_pressure_run": longest_run(states, pressure), "min_run": min_run,
            "verdict": "persistent_footprint_pressure_candidate"
            if longest_run(states, pressure) >= min_run else "observe_only_no_persistent_footprint_pressure",
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_footprint_imbalance_replay", "input": str(args.input),
                      "invalid_lines": invalid_lines, "summary": summarize_records(records, args.min_run),
                      "limitations": [
                          "this replay tests state persistence, not forward returns or profitability",
                          "footprint snapshots come from a bounded rolling in-memory trade buffer",
                          "imbalance is not resting liquidity, liquidation evidence or position ownership",
                          "no order, wallet, allocation or execution path is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
