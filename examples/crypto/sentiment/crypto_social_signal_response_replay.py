#!/usr/bin/env python3
"""Replay absolute BTC movement after recorded social-signal changes."""

import argparse
import json
import statistics
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


def numeric(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def price(record):
    value = numeric(((record.get("observation") or {}).get("price") or {}).get("price"))
    return value if value is not None and value > 0 else None


def social_change(record):
    observation = record.get("observation") or {}
    value = numeric(observation.get("social_change"))
    if value is not None:
        return value
    return numeric(observation.get("signal_change"))


def bucket(change, threshold):
    if change is None:
        return "missing_social"
    if change >= threshold:
        return "social_rise"
    if change <= -threshold:
        return "social_fall"
    return "ordinary"


def stats(values):
    returns = [item[0] for item in values]
    absolute = [item[1] for item in values]
    return {
        "observations": len(values),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_absolute_forward_return_pct": statistics.median(absolute) if absolute else None,
    }


def summarize_records(records, horizon_records, min_observations, min_change):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {"social_rise": [], "social_fall": [], "ordinary": [], "missing_social": []}
    aligned = 0
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline = price(record)
        future = price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        aligned += 1
        forward = (future / baseline - 1.0) * 100.0
        buckets[bucket(social_change(record), min_change)].append((forward, abs(forward)))
    by_state = {state: stats(values) for state, values in buckets.items()}
    qualifying = by_state["social_rise"]["observations"] + by_state["social_fall"]["observations"]
    return {
        "snapshots": len(ordered), "aligned_forward_windows": aligned, "by_state": by_state,
        "verdict": "social_signal_response_reported" if qualifying >= min_observations
        else "observe_only_insufficient_social_changes", "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=24)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--min-change", type=float, default=1.0)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0 or args.min_change < 0:
        parser.error("horizon and observations must be positive; change cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_social_signal_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations, "min_change": args.min_change},
        "summary": summarize_records(records, args.horizon_records,
                                      args.min_observations, args.min_change),
        "limitations": [
            "provider social scores can change metric semantics and scale across sources",
            "keyed external signals are snapshots, not a complete social-history ledger",
            "social changes are non-directional context and do not establish causality",
            "fixed record horizons do not imply exact elapsed time, fees or execution",
        ], "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
