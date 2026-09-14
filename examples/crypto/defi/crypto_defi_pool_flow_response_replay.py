#!/usr/bin/env python3
"""Replay BTC response after DEX-pool pressure snapshots."""

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


def snapshot_state(observation):
    pools = observation.get("pools") or []
    if not pools:
        return "missing_pool_data"
    stress_states = {"thin_liquidity_high_flow", "high_turnover_pool"}
    if any(pool.get("state") in stress_states for pool in pools if isinstance(pool, dict)):
        return "pressure"
    return "ordinary_pool_activity"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns)) if returns else None,
    }


def summarize_records(records, horizon_records, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        current_price = number(((record.get("observation") or {}).get("price") or {}).get("price"))
        future_price = number((((ordered[future_index].get("observation") or {}).get("price") or {}).get("price")))
        if current_price is None or future_price is None or current_price <= 0 or future_price <= 0:
            continue
        observation = record.get("observation") or {}
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": snapshot_state(observation),
            "pool_count": len(observation.get("pools") or []),
            "forward_return_pct": (future_price / current_price - 1.0) * 100.0,
        })
    by_state = {
        state: bucket_stats([row for row in observations if row["state"] == state])
        for state in ("pressure", "ordinary_pool_activity", "missing_pool_data")
    }
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "verdict": ("defi_pool_response_reported" if len(observations) >= min_observations
                    else "observe_only_insufficient_aligned_pool_snapshots"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=1)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_defi_pool_flow_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records, "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "provider snapshots do not establish complete on-chain event coverage",
            "BTC response is descriptive and does not establish causality, LP income or executable price impact",
            "sampling gaps, quote freshness, gas, route depth, wallets and orders are not modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
