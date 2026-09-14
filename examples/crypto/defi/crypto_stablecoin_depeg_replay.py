#!/usr/bin/env python3
"""Replay stablecoin stress versus later absolute risk-asset movement."""

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
                row = json.loads(line)
            except json.JSONDecodeError:
                invalid_lines += 1
                continue
            if isinstance(row, dict) and isinstance(row.get("observation"), dict):
                records.append(row)
            else:
                invalid_lines += 1
    return records, invalid_lines


def event_observations(records, horizon_snapshots, stress_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    stress, ordinary = [], []
    for index, row in enumerate(ordered[:-horizon_snapshots]):
        current = (row.get("observation") or {}).get("risk_asset") or {}
        future = ((ordered[index + horizon_snapshots].get("observation") or {}).get("risk_asset") or {})
        current_price, future_price = current.get("mid"), future.get("mid")
        if not isinstance(current_price, (int, float)) or not isinstance(future_price, (int, float)):
            continue
        if current_price <= 0 or future_price <= 0:
            continue
        move_bps = abs((future_price / current_price - 1.0) * 10_000.0)
        worst = ((row.get("observation") or {}).get("worst_quote") or {})
        deviation = worst.get("abs_deviation_bps")
        target = stress if isinstance(deviation, (int, float)) and deviation >= stress_bps else ordinary
        target.append({"recorded_at_ms": row.get("recorded_at_ms"),
                       "absolute_move_bps": move_bps, "stablecoin_deviation_bps": deviation})
    return stress, ordinary


def summarize_records(records, horizon_snapshots, stress_bps, min_stress, min_ordinary):
    stress, ordinary = event_observations(records, horizon_snapshots, stress_bps)
    stress_moves = [row["absolute_move_bps"] for row in stress]
    ordinary_moves = [row["absolute_move_bps"] for row in ordinary]
    stress_mean = statistics.mean(stress_moves) if stress_moves else None
    ordinary_mean = statistics.mean(ordinary_moves) if ordinary_moves else None
    candidate = (len(stress_moves) >= min_stress and len(ordinary_moves) >= min_ordinary
                 and stress_mean is not None and ordinary_mean is not None
                 and stress_mean > ordinary_mean)
    return {
        "snapshots": len(records), "stress_observations": len(stress_moves),
        "ordinary_observations": len(ordinary_moves),
        "mean_stress_absolute_move_bps": stress_mean,
        "mean_ordinary_absolute_move_bps": ordinary_mean,
        "stress_to_ordinary_move_ratio": stress_mean / ordinary_mean
        if stress_mean is not None and ordinary_mean else None,
        "horizon_snapshots": horizon_snapshots, "stress_bps": stress_bps,
        "verdict": "stablecoin_depeg_contagion_candidate" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-snapshots", type=int, default=3)
    parser.add_argument("--stress-bps", type=float, default=50.0)
    parser.add_argument("--min-stress", type=int, default=3)
    parser.add_argument("--min-ordinary", type=int, default=3)
    args = parser.parse_args()
    if args.horizon_snapshots <= 0 or args.stress_bps < 0 or args.min_stress <= 0 or args.min_ordinary <= 0:
        parser.error("horizon, stress and minimum observations must be positive/non-negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_stablecoin_depeg_replay", "input": str(args.input),
                      "invalid_lines": invalid_lines,
                      "summary": summarize_records(records, args.horizon_snapshots, args.stress_bps,
                                                    args.min_stress, args.min_ordinary),
                      "limitations": [
                          "recorder snapshots are not a complete stablecoin price or liquidity history",
                          "ordinary observations are not a matched causal control sample",
                          "absolute risk-asset movement is non-directional and not a trade result",
                          "no redemption, reserve, route, wallet or execution model is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
