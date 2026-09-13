#!/usr/bin/env python3
"""Replay persistence of recorded DEX-pool flow/liquidity states."""

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
    groups = {}
    for record in sorted(records, key=lambda item: item.get("recorded_at_ms", 0)):
        for pool in (record.get("observation") or {}).get("pools", []):
            source = str(pool.get("source", "")).lower()
            symbol = str(pool.get("symbol", "")).upper()
            if source and symbol:
                groups.setdefault((source, symbol), []).append(pool)

    by_pool = {}
    for (source, symbol), pools in sorted(groups.items()):
        states = [pool.get("state", "observe_only_missing_liquidity_or_volume") for pool in pools]
        stress_states = {"thin_liquidity_high_flow", "high_turnover_pool"}
        stress_count = sum(state in stress_states for state in states)
        persistent = max(
            longest_run(states, "thin_liquidity_high_flow"),
            longest_run(states, "high_turnover_pool"),
        )
        by_pool[f"{source}:{symbol}"] = {
            "observations": len(pools),
            "stress_observations": stress_count,
            "stress_share": stress_count / len(states) if states else None,
            "longest_stress_run": persistent,
            "state_counts": {state: states.count(state) for state in sorted(set(states))},
            "verdict": (
                "persistent_defi_pool_pressure_candidate"
                if persistent >= min_run else "observe_only_no_persistent_pool_pressure"
            ),
        }
    return {
        "pools": len(by_pool),
        "snapshots": len(records),
        "by_pool": by_pool,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(options.input)
    print(json.dumps({
        "strategy": "crypto_defi_pool_flow_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "filters": {"min_run": options.min_run},
        "summary": summarize_records(records, options.min_run),
        "limitations": [
            "provider snapshots do not establish complete on-chain event coverage",
            "turnover persistence is not LP fee income, impermanent-loss or token-return PnL",
            "no swap route, gas, wallet, order or execution model is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
