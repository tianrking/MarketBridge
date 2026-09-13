#!/usr/bin/env python3
"""Replay BTC response after recorded option-skew states."""

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


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def skew_state(target, threshold):
    skew = number((target or {}).get("put_call_skew_iv"))
    if skew is None:
        return "missing_comparable_wings"
    if skew >= threshold:
        return "downside_protection_demand"
    if skew <= -threshold:
        return "upside_call_demand"
    return "balanced_wing_iv"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize_records(records, horizon_records, min_skew_iv, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        current = number((((record.get("observation") or {}).get("price") or {}).get("price")))
        future = number((((ordered[future_index].get("observation") or {}).get("price") or {}).get("price")))
        if current is None or future is None or current <= 0 or future <= 0:
            continue
        observation = record.get("observation") or {}
        target = observation.get("target_expiry") or {}
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": skew_state(target, min_skew_iv),
            "put_call_skew_iv": target.get("put_call_skew_iv"),
            "term_state": (observation.get("term_structure") or {}).get("state"),
            "forward_return_pct": (future / current - 1.0) * 100.0,
        })
    states = ("downside_protection_demand", "upside_call_demand", "balanced_wing_iv",
              "missing_comparable_wings")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    valid = len(observations) - by_state["missing_comparable_wings"]["observations"]
    return {
        "by_state": by_state, "aligned_forward_windows": len(observations),
        "skew_aligned_windows": valid,
        "verdict": ("options_skew_response_reported" if valid >= min_observations
                    else "observe_only_insufficient_aligned_skew_snapshots"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_skew_iv < 0 or args.min_observations <= 0:
        parser.error("horizon and minimum observations must be positive; skew threshold cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_options_skew_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records, "min_skew_iv": args.min_skew_iv,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_skew_iv,
                                      args.min_observations),
        "limitations": [
            "moneyness buckets are not a universal 25-delta skew and expiry identity can roll",
            "mark IV is a venue snapshot, not an executable option quote or option PnL",
            "BTC forward movement is descriptive and does not establish causality, hedgeability or execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
