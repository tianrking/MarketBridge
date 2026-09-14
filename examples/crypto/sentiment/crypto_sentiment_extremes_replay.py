#!/usr/bin/env python3
"""Replay fixed-horizon price responses after sentiment extremes."""

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


def _price(record):
    price = ((record.get("observation") or {}).get("price") or {}).get("price")
    return float(price) if isinstance(price, (int, float)) and price > 0 else None


def _state(record):
    return ((record.get("observation") or {}).get("sentiment") or {}).get(
        "state", "observe_only_missing_or_invalid_sentiment"
    )


def _stats(values, paper_cost_bps):
    after_cost = [value - paper_cost_bps / 100.0 for value in values]
    return {
        "observations": len(values),
        "mean_return_pct": statistics.mean(values) if values else None,
        "median_return_pct": statistics.median(values) if values else None,
        "mean_after_cost_return_pct": statistics.mean(after_cost) if after_cost else None,
        "positive_fraction": (sum(value > 0 for value in after_cost) / len(after_cost))
        if after_cost else None,
    }


def summarize_records(records, horizon_records, min_observations, paper_cost_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    raw_returns = []
    candidate_returns = {"extreme_fear_context": [], "extreme_greed_context": []}
    for index, record in enumerate(ordered):
        baseline = _price(record)
        future_index = index + horizon_records
        future = _price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        forward_return = (future / baseline - 1.0) * 100.0
        raw_returns.append(forward_return)
        state = _state(record)
        if state == "extreme_fear_context":
            candidate_returns[state].append(forward_return)
        elif state == "extreme_greed_context":
            candidate_returns[state].append(-forward_return)
    fear = _stats(candidate_returns["extreme_fear_context"], paper_cost_bps)
    greed = _stats(candidate_returns["extreme_greed_context"], paper_cost_bps)
    candidate_count = fear["observations"] + greed["observations"]
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": len(raw_returns),
        "baseline_forward_return": _stats(raw_returns, 0.0),
        "contrarian_extreme_fear": fear,
        "contrarian_extreme_greed": greed,
        "verdict": "observe_only_insufficient_extreme_observations"
        if candidate_count < min_observations
        else "extreme_sentiment_forward_response_reported",
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
        "strategy": "crypto_sentiment_extremes_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations,
                    "paper_cost_bps": args.paper_cost_bps},
        "summary": summarize_records(records, args.horizon_records,
                                      args.min_observations, args.paper_cost_bps),
        "limitations": [
            "Fear and Greed is a daily provider composite and may repeat between polls",
            "fixed record horizons are only a paper alignment, not a bar-exact fill model",
            "paper cost is a sensitivity input, not venue fees, slippage or execution",
            "no allocation, wallet, order or live execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
