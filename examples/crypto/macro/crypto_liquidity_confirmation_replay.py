#!/usr/bin/env python3
"""Replay BTC responses after multi-channel liquidity context states."""

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


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def price_of(record):
    value = number((((record.get("observation") or {}).get("price") or {}).get("price")))
    return value if value is not None and value > 0 else None


def state_of(record):
    return str((record.get("observation") or {}).get("state") or
               "observe_only_insufficient_liquidity_channels")


def observations(records, horizon_records):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    rows = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        if current is None or future is None:
            continue
        forward = (future / current - 1.0) * 100.0
        rows.append({
            "recorded_at_ms": record.get("recorded_at_ms"),
            "state": state_of(record),
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
            "horizon_recorded_at_ms": ordered[future_index].get("recorded_at_ms"),
        })
    return rows


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "positive_fraction": (sum(value > 0 for value in returns) / len(returns))
        if returns else None,
    }


def summarize_records(records, horizon_records, min_observations):
    rows = observations(records, horizon_records)
    states = (
        "risk_on_confirmation", "liquidity_deterioration", "mixed_liquidity_context",
        "observe_only_insufficient_liquidity_channels",
    )
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    confirmed = (by_state["risk_on_confirmation"]["observations"]
                 + by_state["liquidity_deterioration"]["observations"])
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "confirmed_windows": confirmed,
        "verdict": ("liquidity_confirmation_response_reported"
                    if confirmed >= min_observations
                    else "observe_only_insufficient_confirmation_windows"),
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
        "strategy": "crypto_liquidity_confirmation_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "ETF, stablecoin, Coinbase and funding observations have different clocks and coverage",
            "risk_on and deterioration states are transparent classifications, not causal factors or forecasts",
            "the replay uses fixed recorded snapshots and does not model fees, funding cash flow, hedge, inventory or execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
