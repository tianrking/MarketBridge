#!/usr/bin/env python3
"""Summarize persistence of recorded crypto option skew snapshots."""

import argparse
import json
import statistics
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
            observation = record.get("observation") if isinstance(record, dict) else None
            if isinstance(observation, dict):
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


def skew_state(skew, threshold):
    if not isinstance(skew, (int, float)):
        return "observe_only_missing_comparable_wings"
    if skew >= threshold:
        return "downside_protection_demand"
    if skew <= -threshold:
        return "upside_call_demand"
    return "balanced_wing_iv"


def summarize_records(records, min_skew_iv, min_run):
    by_expiry = {}
    for record in records:
        observation = record["observation"]
        target = observation.get("target_expiry") or {}
        expiry = target.get("expiry_time") or "missing_expiry"
        by_expiry.setdefault(expiry, []).append(record)

    expiry_summaries = {}
    for expiry, group in by_expiry.items():
        targets = [(record["observation"].get("target_expiry") or {}) for record in group]
        skews = [target.get("put_call_skew_iv") for target in targets
                 if isinstance(target.get("put_call_skew_iv"), (int, float))]
        states = [skew_state(target.get("put_call_skew_iv"), min_skew_iv) for target in targets]
        term_states = [(record["observation"].get("term_structure") or {}).get("state", "missing")
                       for record in group]
        downside_count = sum(value >= min_skew_iv for value in skews)
        expiry_summaries[expiry] = {
            "observations": len(group),
            "skew_observations": len(skews),
            "mean_skew_iv": statistics.mean(skews) if skews else None,
            "median_skew_iv": statistics.median(skews) if skews else None,
            "downside_skew_share": downside_count / len(skews) if skews else None,
            "longest_downside_run": longest_run(states, "downside_protection_demand"),
            "longest_upside_run": longest_run(states, "upside_call_demand"),
            "term_state_counts": {state: term_states.count(state) for state in sorted(set(term_states))},
            "verdict": (
                "persistent_downside_skew_candidate"
                if len(skews) >= min_run and downside_count / len(skews) >= 0.5
                and longest_run(states, "downside_protection_demand") >= min_run
                else "observe_only_no_persistent_downside_skew"
            ),
        }
    return {
        "snapshots": len(records),
        "expiries": len(expiry_summaries),
        "by_expiry": expiry_summaries,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.min_skew_iv < 0 or options.min_run <= 0:
        parser.error("min-skew-iv cannot be negative and min-run must be positive")
    records, invalid_lines = load_records(options.input)
    summary = summarize_records(records, options.min_skew_iv, options.min_run)
    print(json.dumps({
        "strategy": "crypto_options_skew_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "summary": summary,
        "limitations": [
            "persistence is descriptive and does not imply option PnL or hedgeability",
            "snapshots can change expiry identity as the target horizon rolls",
            "mark IV, moneyness buckets and stale filtering remain venue-specific",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
