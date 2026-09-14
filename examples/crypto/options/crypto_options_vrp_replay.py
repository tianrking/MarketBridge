#!/usr/bin/env python3
"""Replay persistence of recorded crypto option IV-minus-RV regimes."""

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


def vrp_state(value, threshold):
    if not isinstance(value, (int, float)):
        return "observe_only_missing_iv_or_rv"
    if value >= threshold:
        return "implied_volatility_premium"
    if value <= -threshold:
        return "realized_volatility_above_implied"
    return "implied_and_realized_vol_aligned"


def longest_run(values, target):
    best = current = 0
    for value in values:
        current = current + 1 if value == target else 0
        best = max(best, current)
    return best


def summarize_records(records, threshold, min_run):
    groups = {}
    for record in records:
        observation = record["observation"]
        target = observation.get("target_expiry") or {}
        expiry = target.get("expiry_time") or "missing_expiry"
        groups.setdefault(expiry, []).append(record)

    by_expiry = {}
    for expiry, group in groups.items():
        values = []
        states = []
        spot_closes = []
        for record in group:
            observation = record["observation"]
            vrp = observation.get("vrp") or {}
            value = vrp.get("iv_minus_rv_iv_points")
            if isinstance(value, (int, float)):
                values.append(value)
            states.append(vrp_state(value, threshold))
            realized = observation.get("realized_volatility") or {}
            spot = realized.get("spot_close")
            if isinstance(spot, (int, float)) and spot > 0:
                spot_closes.append(spot)
        premium_run = longest_run(states, "implied_volatility_premium")
        by_expiry[expiry] = {
            "observations": len(group),
            "vrp_observations": len(values),
            "mean_iv_minus_rv_iv_points": statistics.mean(values) if values else None,
            "median_iv_minus_rv_iv_points": statistics.median(values) if values else None,
            "premium_state_share": (
                states.count("implied_volatility_premium") / len(values)
                if values else None
            ),
            "longest_premium_run": premium_run,
            "spot_close_observations": len(spot_closes),
            "verdict": (
                "persistent_vrp_candidate"
                if len(values) >= min_run and premium_run >= min_run
                else "observe_only_no_persistent_vrp"
            ),
        }
    return {
        "snapshots": len(records),
        "expiries": len(by_expiry),
        "by_expiry": by_expiry,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--min-run", type=int, default=3)
    options = parser.parse_args()
    if options.vrp_threshold < 0 or options.min_run <= 0:
        parser.error("vrp-threshold cannot be negative and min-run must be positive")
    records, invalid_lines = load_records(options.input)
    summary = summarize_records(records, options.vrp_threshold, options.min_run)
    print(json.dumps({
        "strategy": "crypto_options_vrp_replay",
        "input": str(options.input),
        "invalid_lines": invalid_lines,
        "filters": {"vrp_threshold": options.vrp_threshold, "min_run": options.min_run},
        "summary": summary,
        "limitations": [
            "IV and RV use different horizons and are not a matched option PnL calculation",
            "persistence is descriptive and does not imply short-volatility profitability",
            "spot_close is retained only for auditability; no delta hedge or execution is modeled",
            "expiry identity can change as the selected horizon rolls",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
