#!/usr/bin/env python3
"""Replay persistence of recorded funding-sensitive yield risk states."""

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
    states = [(row.get("observation") or {}).get("state_counts", {}) for row in ordered]
    risk_states = ["funding_sensitive_yield_risk" in state for state in states]
    risk_run = longest_run(risk_states, True)
    funding_states = [((row.get("observation") or {}).get("funding") or {}).get(
        "state", "observe_only_missing_funding_context") for row in ordered]
    return {
        "snapshots": len(ordered),
        "funding_state_counts": {state: funding_states.count(state) for state in sorted(set(funding_states))},
        "funding_sensitive_snapshot_count": sum(risk_states),
        "funding_sensitive_longest_run": risk_run,
        "verdict": "persistent_funding_sensitive_yield_candidate" if risk_run >= min_run
        else "observe_only_no_persistent_funding_sensitive_yield",
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
        "strategy": "crypto_defi_funding_yield_risk_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"min_run": args.min_run},
        "summary": summarize_records(records, args.min_run),
        "limitations": [
            "provider APY and selected perp funding are asynchronous context snapshots",
            "persistence does not establish causality, protocol revenue or user PnL",
            "no deposit, redemption, allocation, wallet or execution model is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
