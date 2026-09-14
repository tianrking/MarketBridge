#!/usr/bin/env python3
"""Replay persistence of Meteora DLMM fee/turnover states."""

import argparse
import json
from pathlib import Path


def load_records(path):
    records, invalid = [], 0
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                invalid += 1
                continue
            if isinstance(row, dict) and isinstance(row.get("observation"), dict):
                records.append(row)
            else:
                invalid += 1
    return records, invalid


def longest_run(states, targets):
    best = current = 0
    for state in states:
        current = current + 1 if state in targets else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run):
    groups = {}
    for record in sorted(records, key=lambda item: item.get("recorded_at_ms", 0)):
        for pool in (record.get("observation") or {}).get("pools", []):
            source, symbol = str(pool.get("source", "")).lower(), str(pool.get("symbol", "")).upper()
            if source and symbol:
                groups.setdefault((source, symbol), []).append(pool)
    stress = {"high_fee_turnover_dlmm", "high_turnover_dlmm"}
    by_pool = {}
    for (source, symbol), pools in sorted(groups.items()):
        states = [pool.get("state", "observe_only_missing_meteora_metrics") for pool in pools]
        count = sum(state in stress for state in states)
        run = longest_run(states, stress)
        by_pool[f"{source}:{symbol}"] = {
            "observations": len(states), "stress_observations": count,
            "stress_share": count / len(states) if states else None, "longest_stress_run": run,
            "state_counts": {state: states.count(state) for state in sorted(set(states))},
            "verdict": ("persistent_meteora_dlmm_pressure_candidate" if run >= min_run
                        else "observe_only_no_persistent_meteora_dlmm_pressure"),
        }
    return {"pools": len(by_pool), "snapshots": len(records), "by_pool": by_pool, "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid = load_records(args.input)
    print(json.dumps({"strategy": "crypto_meteora_dlmm_replay", "input": str(args.input),
                      "invalid_lines": invalid, "summary": summarize_records(records, args.min_run),
                      "limitations": ["snapshot persistence is not causal alpha, LP income or execution PnL",
                                      "partial pages, active bins, gas, MEV, wallets and orders remain unmodeled"],
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
