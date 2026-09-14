#!/usr/bin/env python3
"""Replay BTC response after recorded short-squeeze confluence candidates."""

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


def price(record):
    value = ((record.get("observation") or {}).get("price") or {}).get("price")
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0 else None


def stats(returns, paper_cost_bps):
    adjusted = [value - paper_cost_bps / 100.0 for value in returns]
    return {
        "observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
        "positive_fraction_after_cost": sum(value > 0 for value in adjusted) / len(adjusted)
        if adjusted else None,
        "mean_after_cost_return_pct": statistics.mean(adjusted) if adjusted else None,
    }


def summarize_records(records, horizon_records, min_observations, min_score, paper_cost_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {"candidate": [], "observe_only": []}
    aligned = 0
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline = price(record)
        future = price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        aligned += 1
        score = ((record.get("observation") or {}).get("score"))
        key = "candidate" if isinstance(score, (int, float)) and score >= min_score else "observe_only"
        buckets[key].append((future / baseline - 1.0) * 100.0)
    return {
        "snapshots": len(ordered), "aligned_forward_windows": aligned,
        "by_state": {key: stats(values, paper_cost_bps) for key, values in buckets.items()},
        "verdict": "short_squeeze_response_reported" if aligned >= min_observations
        else "observe_only_insufficient_squeeze_observations", "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--min-score", type=int, default=3)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.min_observations <= 0
            or args.min_score <= 0 or args.paper_cost_bps < 0):
        parser.error("horizon, score and observations must be positive; cost cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_short_squeeze_response_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_observations": args.min_observations,
                                  "min_score": args.min_score,
                                  "paper_cost_bps": args.paper_cost_bps},
                      "summary": summarize_records(records, args.horizon_records,
                                                    args.min_observations, args.min_score,
                                                    args.paper_cost_bps),
                      "limitations": [
                          "short-squeeze score is a four-component current snapshot and OI first poll has no baseline",
                          "fixed record horizons do not imply exact elapsed time, fills or causality",
                          "single-venue flow and liquidation semantics are provider-specific",
                          "paper cost is a sensitivity input, not fees, funding, slippage or execution",
                      ], "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
