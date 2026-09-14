#!/usr/bin/env python3
"""Replay persistence of recorded unsigned option gamma concentration."""

import argparse
import json
from pathlib import Path

from crypto_options_gamma_monitor import classify_gamma


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


def summarize_records(records, min_near_share, min_concentration, min_run):
    targets = [record["observation"].get("target_expiry") or {} for record in records]
    states = [classify_gamma(target, min_near_share, min_concentration) for target in targets]
    near_shares = [target.get("near_spot_share") for target in targets
                   if isinstance(target.get("near_spot_share"), (int, float))]
    concentrations = [target.get("concentration_at_strike") for target in targets
                      if isinstance(target.get("concentration_at_strike"), (int, float))]
    concentrated = states.count("near_spot_gamma_concentration")
    return {
        "snapshots": len(records),
        "gamma_snapshots": len(near_shares),
        "mean_near_spot_share": sum(near_shares) / len(near_shares) if near_shares else None,
        "mean_concentration_at_strike": sum(concentrations) / len(concentrations) if concentrations else None,
        "state_counts": {state: states.count(state) for state in sorted(set(states))},
        "longest_near_spot_concentration_run": longest_run(states, "near_spot_gamma_concentration"),
        "verdict": (
            "persistent_near_spot_gamma_map"
            if concentrated >= min_run and longest_run(states, "near_spot_gamma_concentration") >= min_run
            else "observe_only_no_persistent_gamma_map"
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-near-share", type=float, default=0.50)
    parser.add_argument("--min-concentration", type=float, default=0.10)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if (not 0 <= options.min_near_share <= 1
            or not 0 <= options.min_concentration <= 1 or options.min_run <= 0):
        parser.error("thresholds must be in [0, 1] and min-run must be positive")
    records, invalid_lines = load_records(options.input)
    print(json.dumps({
        "strategy": "crypto_options_gamma_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "summary": summarize_records(records, options.min_near_share,
                                      options.min_concentration, options.min_run),
        "limitations": [
            "persistence is descriptive and does not infer dealer gamma sign",
            "no realized-volatility response, hedge, execution or option PnL simulation",
            "target expiry can roll as the nearest maturity changes",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
