#!/usr/bin/env python3
"""Replay forward absolute price movement after open-interest impulses."""

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
    value = ((record.get("observation") or {}).get("price") or {}).get("price")
    value = numeric(value)
    return value if value is not None and value > 0 else None


def oi_change(record):
    value = numeric((record.get("observation") or {}).get("oi_change_pct"))
    if value is not None:
        return value
    return numeric((record.get("observation") or {}).get("open_interest_change_pct"))


def bucket(change, threshold):
    if change is None:
        return "missing_oi"
    if change >= threshold:
        return "oi_expansion"
    if change <= -threshold:
        return "oi_contraction"
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


def summarize_records(records, horizon_records, min_observations, min_oi_change_pct):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {"oi_expansion": [], "oi_contraction": [], "ordinary": [], "missing_oi": []}
    aligned = 0
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline, future = price(record), price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        aligned += 1
        forward = (future / baseline - 1.0) * 100.0
        state = bucket(oi_change(record), min_oi_change_pct)
        buckets[state].append((forward, abs(forward)))
    by_state = {state: stats(values) for state, values in buckets.items()}
    expansion_count = by_state["oi_expansion"]["observations"]
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": aligned,
        "by_state": by_state,
        "verdict": "oi_impulse_response_reported" if expansion_count >= min_observations
        else "observe_only_insufficient_oi_impulses",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--min-oi-change-pct", type=float, default=0.25)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.min_observations <= 0
            or args.min_oi_change_pct < 0):
        parser.error("horizon and observations must be positive; OI threshold cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_oi_impulse_response_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations,
                    "min_oi_change_pct": args.min_oi_change_pct},
        "summary": summarize_records(records, args.horizon_records,
                                      args.min_observations, args.min_oi_change_pct),
        "limitations": [
            "OI is aggregate positioning and does not identify long/short ownership",
            "OI expansion is tested as a volatility/liquidation-risk hypothesis, not a direction signal",
            "fixed record horizons do not imply exact elapsed time or causality",
            "paper replay has no fees, funding, fills, leverage or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
