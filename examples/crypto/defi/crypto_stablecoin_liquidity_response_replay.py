#!/usr/bin/env python3
"""Replay BTC responses after stablecoin supply expansion/contraction states."""

import argparse
import json
import statistics
from pathlib import Path

from crypto_stablecoin_liquidity_monitor import summarize


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
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def price_of(record):
    value = number((((record.get("observation") or {}).get("price") or {}).get("price")))
    return value if value is not None and value > 0 else None


def state_of(record, growth_threshold_pct):
    observation = record.get("observation") or {}
    stablecoins = observation.get("stablecoins") or {}
    state = summarize(stablecoins, growth_threshold_pct).get("state")
    return state or "observe_only_missing_supply_change"


def observations(records, horizon_records, growth_threshold_pct):
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
            "state": state_of(record, growth_threshold_pct),
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
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_absolute_forward_return_pct": statistics.median(absolute) if absolute else None,
    }


def summarize_records(records, horizon_records, growth_threshold_pct, min_observations):
    rows = observations(records, horizon_records, growth_threshold_pct)
    states = ("stablecoin_supply_expansion", "stablecoin_supply_contraction", "stablecoin_supply_flat")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    expansion = by_state["stablecoin_supply_expansion"]["mean_absolute_forward_return_pct"]
    contraction = by_state["stablecoin_supply_contraction"]["mean_absolute_forward_return_pct"]
    edge_bps = ((expansion - contraction) * 100.0
                if expansion is not None and contraction is not None else None)
    qualifying = (by_state["stablecoin_supply_expansion"]["observations"]
                  + by_state["stablecoin_supply_contraction"]["observations"])
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "expansion_vs_contraction_absolute_move_edge_bps": edge_bps,
        "verdict": ("stablecoin_liquidity_response_reported"
                    if qualifying >= min_observations
                    else "observe_only_insufficient_supply_regime_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--growth-threshold-pct", type=float, default=1.0)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.growth_threshold_pct < 0
            or args.min_observations <= 0):
        parser.error("horizon, threshold and min-observations must be valid")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, args.horizon_records, args.growth_threshold_pct,
                                args.min_observations)
    print(json.dumps({
        "strategy": "crypto_stablecoin_liquidity_response_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "growth_threshold_pct": args.growth_threshold_pct,
                    "min_observations": args.min_observations},
        "summary": summary,
        "limitations": [
            "circulating supply is not exchange inventory, bridge flow or deployable liquidity",
            "DefiLlama snapshots and seven-day changes do not establish causality or price direction",
            "the replay compares fixed-record association; no reserve, redemption, borrow, fee or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
