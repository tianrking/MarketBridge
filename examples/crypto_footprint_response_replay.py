#!/usr/bin/env python3
"""Replay forward price response after footprint bid/ask pressure."""

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


def price(record):
    return number(((record.get("observation") or {}).get("price") or {}).get("price"))


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    aligned = [row["direction_sign"] * value for row, value in zip(rows, returns)]
    return {"observations": len(rows),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "median_forward_return_pct": statistics.median(returns) if returns else None,
            "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
            "aligned_hit_rate": (sum(value > 0 for value in aligned) / len(aligned)) if aligned else None,
            "mean_aligned_return_bps": statistics.mean(aligned) * 100.0 if aligned else None}


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {"bid_pressure": [], "ask_pressure": [], "ordinary_pressure": []}
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline = price(record)
        future = price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or baseline <= 0 or future is None or future <= 0:
            continue
        footprint = ((record.get("observation") or {}).get("footprint") or {})
        state = footprint.get("state", "observe_only_missing_footprint")
        bucket = ("bid_pressure" if state == "footprint_bid_pressure"
                  else "ask_pressure" if state == "footprint_ask_pressure"
                  else "ordinary_pressure")
        buckets[bucket].append({"forward_return_pct": (future / baseline - 1.0) * 100.0,
                                "direction_sign": 1 if bucket == "bid_pressure" else -1 if bucket == "ask_pressure" else 0})
    pressure_count = len(buckets["bid_pressure"]) + len(buckets["ask_pressure"])
    return {"snapshots": len(ordered), "aligned_forward_windows": sum(len(rows) for rows in buckets.values()),
            "by_state": {key: stats(rows) for key, rows in buckets.items()},
            "verdict": "footprint_response_reported" if pressure_count >= min_observations
            else "observe_only_insufficient_footprint_pressure",
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_footprint_response_replay", "input": str(args.input),
                      "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_observations": args.min_observations},
                      "summary": summarize_records(records, args.horizon_records, args.min_observations),
                      "limitations": [
                          "footprint pressure is a bounded rolling trade-buffer state, not resting liquidity",
                          "bid/ask pressure direction is descriptive and does not establish causality or position ownership",
                          "fixed record horizons are not exact elapsed time; missing quotes are excluded",
                          "no fill, fee, funding, wallet or execution model is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
