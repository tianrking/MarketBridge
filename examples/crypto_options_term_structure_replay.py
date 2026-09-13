#!/usr/bin/env python3
"""Replay persistence of recorded crypto option IV term-structure states.

The falsifiable hypothesis is intentionally narrow: when the near/far ATM IV
slope is unusually positive (contango) or negative (backwardation), does that
state persist for a configurable number of observations?  This is a surface
regime diagnostic, not a calendar-spread, option or hedge recommendation.
"""

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
            if isinstance(record, dict) and isinstance(record.get("observation"), dict):
                records.append(record)
            else:
                invalid_lines += 1
    return records, invalid_lines


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def classify_term_structure(observation, min_slope_iv):
    term = observation.get("term_structure") or {}
    slope = number(term.get("atm_iv_slope_iv"))
    if slope is None:
        return "observe_only_missing_term_points"
    if slope >= min_slope_iv:
        return "upward_iv_term_structure"
    if slope <= -min_slope_iv:
        return "inverted_iv_term_structure"
    return "flat_iv_term_structure"


def longest_run(values, target):
    best = current = 0
    for value in values:
        current = current + 1 if value == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_slope_iv, min_run):
    ordered = sorted(records, key=lambda record: record.get("recorded_at_ms", 0))
    slopes = []
    states = []
    for record in ordered:
        observation = record["observation"]
        slope = number((observation.get("term_structure") or {}).get("atm_iv_slope_iv"))
        if slope is not None:
            slopes.append(slope)
        states.append(classify_term_structure(observation, min_slope_iv))

    up_state = "upward_iv_term_structure"
    inverted_state = "inverted_iv_term_structure"
    up_run = longest_run(states, up_state)
    inverted_run = longest_run(states, inverted_state)
    up_count = states.count(up_state)
    inverted_count = states.count(inverted_state)
    verdict = "observe_only_no_persistent_term_structure"
    if up_run >= min_run and up_count >= min_run:
        verdict = "persistent_upward_iv_term_structure_candidate"
    elif inverted_run >= min_run and inverted_count >= min_run:
        verdict = "persistent_inverted_iv_term_structure_candidate"

    return {
        "snapshots": len(ordered),
        "slope_observations": len(slopes),
        "mean_atm_iv_slope_iv": statistics.mean(slopes) if slopes else None,
        "median_atm_iv_slope_iv": statistics.median(slopes) if slopes else None,
        "state_counts": {state: states.count(state) for state in sorted(set(states))},
        "longest_upward_run": up_run,
        "longest_inverted_run": inverted_run,
        "verdict": verdict,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-slope-iv", type=float, default=3.0)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.min_slope_iv < 0 or options.min_run <= 0:
        parser.error("min-slope-iv cannot be negative and min-run must be positive")
    records, invalid_lines = load_records(options.input)
    summary = summarize_records(records, options.min_slope_iv, options.min_run)
    print(json.dumps({
        "strategy": "crypto_options_term_structure_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "filters": {"min_slope_iv": options.min_slope_iv, "min_run": options.min_run},
        "summary": summary,
        "limitations": [
            "near and far expiry identities can roll between snapshots",
            "mark IV snapshots are not executable calendar-spread quotes",
            "persistence does not imply option PnL, hedgeability or a direction",
            "no carry, transaction cost, margin or settlement model is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
