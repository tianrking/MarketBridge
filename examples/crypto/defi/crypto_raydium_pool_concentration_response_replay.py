#!/usr/bin/env python3
"""Compare BTC responses after Raydium concentration/turnover states."""

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
    states = {pool.get("state") for pool in pools if isinstance(pool, dict)}
    if "high_turnover_fragmented" in states:
        return "fragmented_pressure"
    if "high_turnover_concentrated" in states:
        return "concentrated_pressure"
    if "ordinary_pool_state" in states:
        return "ordinary_pool_state"
    if "observe_only_partial_pool_catalog" in states:
        return "partial_pool_catalog"
    return "missing_pool_data"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    return {
        "observations": len(returns),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
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
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": snapshot_state(record.get("observation") or {}),
            "forward_return_pct": (future_price / current_price - 1.0) * 100.0,
        })
    states = ("fragmented_pressure", "concentrated_pressure", "ordinary_pool_state",
              "partial_pool_catalog", "missing_pool_data")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(observations),
        "verdict": ("raydium_pool_response_reported" if len(observations) >= min_observations
                    else "observe_only_insufficient_aligned_raydium_snapshots"),
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
        "strategy": "crypto_raydium_pool_concentration_response_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "Raydium pool states are provider snapshots and do not establish causality",
            "BTC response is descriptive; no LP income, slippage, fill or arbitrage PnL is inferred",
            "sampling gaps, price freshness, gas, MEV, wallets and orders are not modeled",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
