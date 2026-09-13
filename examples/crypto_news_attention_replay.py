#!/usr/bin/env python3
"""Replay absolute price movement after CryptoPanic attention shocks."""

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


def _price(record):
    price = ((record.get("observation") or {}).get("price") or {}).get("price")
    return float(price) if isinstance(price, (int, float)) and price > 0 else None


def _state(record):
    return ((record.get("observation") or {}).get("news") or {}).get(
        "state", "observe_only_missing_news"
    )


def _stats(values):
    return {
        "observations": len(values),
        "mean_abs_return_pct": statistics.mean(values) if values else None,
        "median_abs_return_pct": statistics.median(values) if values else None,
    }


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    all_moves, shock_moves = [], []
    for index, record in enumerate(ordered):
        baseline = _price(record)
        future_index = index + horizon_records
        future = _price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        movement = abs(future / baseline - 1.0) * 100.0
        all_moves.append(movement)
        if _state(record) == "news_attention_shock":
            shock_moves.append(movement)
    baseline = _stats(all_moves)
    shock = _stats(shock_moves)
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": len(all_moves),
        "baseline": baseline,
        "attention_shock": shock,
        "mean_excess_abs_return_pct": (
            shock["mean_abs_return_pct"] - baseline["mean_abs_return_pct"]
            if shock["mean_abs_return_pct"] is not None and baseline["mean_abs_return_pct"] is not None
            else None
        ),
        "verdict": "observe_only_insufficient_news_shocks"
        if shock["observations"] < min_observations
        else "news_attention_forward_response_reported",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=6)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon and minimum observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_news_attention_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "CryptoPanic coverage is a bounded provider feed and requires its configured API key",
            "attention shock is non-directional and absolute movement is not a trading edge",
            "fixed record horizons are a paper alignment, not a timestamp-exact fill model",
            "no allocation, wallet, order or live execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
