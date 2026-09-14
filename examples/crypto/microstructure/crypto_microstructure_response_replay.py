#!/usr/bin/env python3
"""Replay price response after order-book imbalance/funding states."""

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


def price_of(record):
    value = number((((record.get("observation") or {}).get("response_price") or {})
                    .get("price")))
    return value if value is not None and value > 0 else None


def direction_sign(state):
    if state in ("bid_pressure_candidate", "bid_pressure_with_long_crowding_conflict"):
        return 1
    if state in ("ask_pressure_candidate", "ask_pressure_with_short_crowding_conflict"):
        return -1
    return 0


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    aligned = [row["direction_sign"] * row["forward_return_pct"]
               for row in rows if row["direction_sign"]]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "aligned_hit_rate": (sum(value > 0 for value in aligned) / len(aligned)
                              if aligned else None),
        "mean_aligned_return_bps": statistics.mean(aligned) * 100.0 if aligned else None,
    }


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    names = ("bid_pressure_candidate", "ask_pressure_candidate",
             "bid_pressure_with_long_crowding_conflict",
             "ask_pressure_with_short_crowding_conflict", "balanced_book", "observe_only")
    buckets = {name: [] for name in names}
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        if current is None or future is None:
            continue
        raw_state = (record.get("observation") or {}).get("signal", "observe_only")
        state = raw_state if raw_state in buckets else "observe_only"
        forward = (future / current - 1.0) * 100.0
        buckets[state].append({"forward_return_pct": forward,
                               "direction_sign": direction_sign(state)})
    by_state = {name: stats(rows) for name, rows in buckets.items()}
    pressure_count = sum(by_state[name]["observations"] for name in names[:4])
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": sum(len(rows) for rows in buckets.values()),
        "by_state": by_state,
        "verdict": ("microstructure_response_reported" if pressure_count >= min_observations
                    else "observe_only_insufficient_pressure_states"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_microstructure_response_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_observations": args.min_observations},
                      "summary": summarize_records(records, args.horizon_records,
                                                    args.min_observations),
                      "limitations": [
                          "imbalance is a top-level snapshot and funding is crowding context",
                          "fixed record horizons are not exact elapsed time; missing quotes are excluded",
                          "signed alignment is descriptive and does not establish causality or fills",
                          "no fees, queue, latency, liquidation, slippage or execution model",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
