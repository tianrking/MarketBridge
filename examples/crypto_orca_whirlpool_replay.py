#!/usr/bin/env python3
"""Replay persistence of Orca Whirlpool warning/turnover states."""

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
            source = str(pool.get("source", "")).lower()
            symbol = str(pool.get("symbol", "")).upper()
            if source and symbol:
                groups.setdefault((source, symbol), []).append(pool)
    stress_states = {"warning_or_adaptive_fee_pressure", "high_turnover_whirlpool"}
    by_pool = {}
    for (source, symbol), pools in sorted(groups.items()):
        states = [pool.get("state", "observe_only_missing_orca_metrics") for pool in pools]
        stress_count = sum(state in stress_states for state in states)
        run = longest_run(states, stress_states)
        by_pool[f"{source}:{symbol}"] = {
            "observations": len(pools),
            "stress_observations": stress_count,
            "stress_share": stress_count / len(states) if states else None,
            "longest_stress_run": run,
            "state_counts": {state: states.count(state) for state in sorted(set(states))},
            "verdict": ("persistent_orca_whirlpool_pressure_candidate" if run >= min_run
                        else "observe_only_no_persistent_orca_whirlpool_pressure"),
        }
    return {"pools": len(by_pool), "snapshots": len(records), "by_pool": by_pool,
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_orca_whirlpool_replay", "input": str(args.input),
                      "invalid_lines": invalid_lines, "filters": {"min_run": args.min_run},
                      "summary": summarize_records(records, args.min_run),
                      "limitations": [
                          "Orca REST snapshots do not establish complete on-chain swap coverage",
                          "warning/adaptive-fee persistence is not LP income, slippage or execution PnL",
                          "no active tick-range, gas, MEV, wallet, transaction or order model is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
