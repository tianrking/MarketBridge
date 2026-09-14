#!/usr/bin/env python3
"""Replay fixed-record-horizon price response after crowding snapshots."""

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


def price(record):
    value = ((record.get("observation") or {}).get("price") or {}).get("price")
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0 else None


def state(record):
    return ((record.get("observation") or {}).get("summary") or {}).get(
        "state", "observe_only_missing_positioning_metrics")


def liquidation_active(record):
    return bool(((record.get("observation") or {}).get("summary") or {}).get(
        "liquidation_activity", False))


def stats(values, paper_cost_bps):
    after_cost = [value - paper_cost_bps / 100.0 for value in values]
    return {
        "observations": len(values),
        "mean_signed_return_pct": statistics.mean(values) if values else None,
        "median_signed_return_pct": statistics.median(values) if values else None,
        "mean_after_cost_signed_return_pct": statistics.mean(after_cost) if after_cost else None,
        "positive_fraction_after_cost": (sum(value > 0 for value in after_cost) / len(after_cost))
        if after_cost else None,
    }


def summarize_records(records, horizon_records, min_observations, paper_cost_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    raw_returns = []
    buckets = {
        "long_crowding_context": [],
        "short_crowding_context": [],
        "long_crowding_with_liquidation": [],
        "short_crowding_with_liquidation": [],
    }
    aligned_windows = 0
    time_gaps_ms = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline = price(record)
        future = price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        aligned_windows += 1
        forward_return = (future / baseline - 1.0) * 100.0
        raw_returns.append(forward_return)
        if future_index < len(ordered):
            time_gaps_ms.append(ordered[future_index].get("recorded_at_ms", 0)
                                - record.get("recorded_at_ms", 0))
        current_state = state(record)
        if current_state == "long_crowding_context":
            buckets["long_crowding_context"].append(-forward_return)
            if liquidation_active(record):
                buckets["long_crowding_with_liquidation"].append(-forward_return)
        elif current_state == "short_crowding_context":
            buckets["short_crowding_context"].append(forward_return)
            if liquidation_active(record):
                buckets["short_crowding_with_liquidation"].append(forward_return)
    bucket_stats = {key: stats(values, paper_cost_bps) for key, values in buckets.items()}
    candidate_count = (bucket_stats["long_crowding_context"]["observations"]
                       + bucket_stats["short_crowding_context"]["observations"])
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": aligned_windows,
        "median_record_gap_ms": statistics.median(time_gaps_ms) if time_gaps_ms else None,
        "baseline_forward_return": stats(raw_returns, 0.0),
        "contrarian_buckets": bucket_stats,
        "verdict": "crowding_forward_response_reported"
        if candidate_count >= min_observations
        else "observe_only_insufficient_crowding_observations",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0 or args.paper_cost_bps < 0:
        parser.error("horizon and minimum observations must be positive; cost cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_derivatives_crowding_response_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations,
                    "paper_cost_bps": args.paper_cost_bps},
        "summary": summarize_records(records, args.horizon_records,
                                      args.min_observations, args.paper_cost_bps),
        "limitations": [
            "CoinGlass metrics are provider snapshots, not position ownership or a complete market ledger",
            "record-count horizons are paper alignment and do not imply exact elapsed time or fills",
            "contrarian returns are a signed descriptive transform, not a trade recommendation",
            "paper cost is a sensitivity input, not venue fees, slippage, borrow or hedge execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
