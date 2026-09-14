#!/usr/bin/env python3
"""Compare fixed-record BTC responses after Meteora DLMM states."""

import argparse
import json
import statistics
from pathlib import Path

from crypto_meteora_dlmm_replay import load_records


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def snapshot_state(observation):
    states = {pool.get("state") for pool in observation.get("pools", []) if isinstance(pool, dict)}
    for state in ("high_fee_turnover_dlmm", "high_turnover_dlmm", "blacklisted_pool_context",
                  "ordinary_dlmm_state", "observe_only_partial_meteora_page"):
        if state in states:
            return state
    return "missing_pool_data"


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    rows = []
    for index, record in enumerate(ordered[:-horizon_records]):
        current = number(((record.get("observation") or {}).get("price") or {}).get("price"))
        future = number((((ordered[index + horizon_records].get("observation") or {}).get("price") or {}).get("price")))
        if current and future and current > 0 and future > 0:
            rows.append({"state": snapshot_state(record.get("observation") or {}),
                         "forward_return_pct": (future / current - 1.0) * 100.0})
    states = ("high_fee_turnover_dlmm", "high_turnover_dlmm", "blacklisted_pool_context",
              "ordinary_dlmm_state", "observe_only_partial_meteora_page", "missing_pool_data")
    by_state = {}
    for state in states:
        values = [row["forward_return_pct"] for row in rows if row["state"] == state]
        by_state[state] = {"observations": len(values),
                           "mean_forward_return_pct": statistics.mean(values) if values else None,
                           "mean_absolute_forward_return_pct": statistics.mean(abs(x) for x in values) if values else None,
                           "positive_fraction": sum(x > 0 for x in values) / len(values) if values else None}
    return {"by_state": by_state, "aligned_forward_windows": len(rows),
            "verdict": ("meteora_dlmm_response_reported" if len(rows) >= min_observations
                        else "observe_only_insufficient_aligned_meteora_snapshots"), "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=1)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid = load_records(args.input)
    print(json.dumps({"strategy": "crypto_meteora_dlmm_response_replay", "input": str(args.input),
                      "invalid_lines": invalid, "summary": summarize_records(records, args.horizon_records, args.min_observations),
                      "limitations": ["fixed-record BTC response is descriptive and not causal", "no LP PnL, fills, swaps, wallets or orders are modeled"],
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
