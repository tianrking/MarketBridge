#!/usr/bin/env python3
"""Replay BTC movement after recorded spot/perp depth-gap states."""

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


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    names = ("perp_depth_advantage_observation", "spot_depth_advantage_observation",
             "no_material_depth_gap", "observe_only")
    buckets = {name: [] for name in names}
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        if current is None or future is None:
            continue
        raw_state = (record.get("observation") or {}).get("state", "observe_only")
        bucket = raw_state if raw_state in buckets else "observe_only"
        forward = (future / current - 1.0) * 100.0
        buckets[bucket].append({
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
        })
    by_state = {name: stats(rows) for name, rows in buckets.items()}
    perp_abs = by_state["perp_depth_advantage_observation"]["mean_absolute_forward_return_pct"]
    normal_abs = by_state["no_material_depth_gap"]["mean_absolute_forward_return_pct"]
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": sum(len(rows) for rows in buckets.values()),
        "by_state": by_state,
        "perp_advantage_absolute_move_edge_bps": (
            (perp_abs - normal_abs) * 100.0
            if perp_abs is not None and normal_abs is not None else None
        ),
        "verdict": (
            "spot_perp_depth_gap_response_reported"
            if (by_state["perp_depth_advantage_observation"]["observations"]
                + by_state["spot_depth_advantage_observation"]["observations"]
                >= min_observations)
            else "observe_only_insufficient_depth_gap_states"
        ),
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
    print(json.dumps({
        "strategy": "crypto_spot_perp_depth_gap_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "depth states are snapshots, not synchronized executable fills or hedge routes",
            "fixed record horizons are not exact elapsed time and missing quotes are excluded",
            "response differences do not establish depth-to-price causality or arbitrage PnL",
            "no fees, latency, queue position, inventory, basis or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
