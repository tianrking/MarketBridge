#!/usr/bin/env python3
"""Replay persistence and data coverage of recorded liquidity stress."""

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


def summarize_records(records, min_run):
    states = [record["observation"].get("state", "observe_only_missing") for record in records]
    valid_inputs = 0
    for record in records:
        observation = record["observation"]
        book = observation.get("book") or {}
        metrics = book.get("metrics") or {}
        if (observation.get("ewma_volatility_bps_per_bar") is not None
                and metrics.get("spread_bps") is not None
                and (metrics.get("buy_impact_bps") is not None
                     or metrics.get("sell_impact_bps") is not None)):
            valid_inputs += 1
    stress_count = states.count("liquidity_stress")
    watch_count = states.count("liquidity_watch")
    return {
        "snapshots": len(records),
        "valid_input_snapshots": valid_inputs,
        "valid_input_fraction": valid_inputs / len(records) if records else None,
        "stress_snapshots": stress_count,
        "watch_snapshots": watch_count,
        "stress_fraction": stress_count / len(records) if records else None,
        "longest_stress_run": longest_run(states, "liquidity_stress"),
        "longest_watch_run": longest_run(states, "liquidity_watch"),
        "state_counts": {state: states.count(state) for state in sorted(set(states))},
        "verdict": (
            "persistent_liquidity_stress_candidate"
            if valid_inputs >= min_run and longest_run(states, "liquidity_stress") >= min_run
            else "observe_only_no_persistent_stress"
        ),
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
        "strategy": "crypto_liquidity_stress_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "filters": {"min_run": options.min_run},
        "summary": summarize_records(records, options.min_run),
        "limitations": [
            "stress is a snapshot risk context, not a directional signal",
            "book impact assumes displayed levels are executable and synchronized",
            "missing depth, spread or volatility remains outside valid-input coverage",
            "no fees, latency, queue position, liquidation, routing or sizing model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
