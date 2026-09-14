#!/usr/bin/env python3
"""Replay persistence of recorded bull-call-spread quote geometry."""

import argparse
import json
import statistics
from pathlib import Path


VALID_STATES = {"bull_call_spread_quote_available", "bull_call_spread_mark_only"}


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


def longest_run(values, target=True):
    best = current = 0
    for value in values:
        current = current + 1 if value == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run):
    groups = {}
    for record in records:
        target = record["observation"].get("target_expiry") or {}
        legs = target.get("legs") or {}
        long_leg = legs.get("long_call") or {}
        short_leg = legs.get("short_call") or {}
        identity = (target.get("expiry_time"), long_leg.get("strike"), short_leg.get("strike"))
        groups.setdefault(identity, []).append(record)
    summaries = {}
    for identity, group in groups.items():
        targets = [(record["observation"].get("target_expiry") or {}) for record in group]
        debits = [target.get("debit") for target in targets if isinstance(target.get("debit"), (int, float))]
        widths = [target.get("width") for target in targets if isinstance(target.get("width"), (int, float))]
        valid = [target.get("state") in VALID_STATES for target in targets]
        key = "|".join("missing" if value is None else str(value) for value in identity)
        summaries[key] = {
            "observations": len(group),
            "valid_observations": sum(valid),
            "mean_debit": statistics.mean(debits) if debits else None,
            "median_debit": statistics.median(debits) if debits else None,
            "mean_width": statistics.mean(widths) if widths else None,
            "longest_valid_run": longest_run(valid),
            "verdict": "persistent_bull_call_spread_quote_candidate"
            if sum(valid) >= min_run and longest_run(valid) >= min_run
            else "observe_only_no_persistent_spread_quote",
        }
    return {"snapshots": len(records), "spread_identities": len(summaries),
            "by_spread": summaries, "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(options.input)
    print(json.dumps({"strategy": "crypto_options_bull_call_spread_replay",
                      "input": str(options.input), "invalid_lines": invalid_lines,
                      "summary": summarize_records(records, options.min_run),
                      "limitations": [
                          "persistence is descriptive and does not imply option PnL or fills",
                          "expiry and strike identity can roll as the target selection changes",
                          "mark-only observations are retained but are not executable two-sided quotes",
                          "no settlement, margin, early-exercise, hedge or transaction-cost model",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
