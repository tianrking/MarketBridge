#!/usr/bin/env python3
"""Replay BTC responses after miner-stress, tailwind and ordinary states."""

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
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def price_of(record):
    value = number((((record.get("observation") or {}).get("price") or {}).get("price")))
    return value if value is not None and value > 0 else None


def classify(data, stress_difficulty, stress_hashrate, tailwind_difficulty, tailwind_hashrate):
    if not isinstance(data, dict):
        return "observe_only_missing_mining_metrics"
    difficulty = number(data.get("difficulty_change_pct"))
    hashrate = number(data.get("hashrate_change_7d_pct"))
    if difficulty is None or hashrate is None:
        return "observe_only_missing_mining_metrics"
    if difficulty <= stress_difficulty or hashrate <= stress_hashrate:
        return "miner_stress_context"
    if difficulty >= tailwind_difficulty and hashrate >= tailwind_hashrate:
        return "miner_tailwind_context"
    return "ordinary_mining_context"


def observations(records, horizon_records, stress_difficulty, stress_hashrate,
                 tailwind_difficulty, tailwind_hashrate):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    rows = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        if current is None or future is None:
            continue
        state = classify((record.get("observation") or {}).get("mining"), stress_difficulty,
                         stress_hashrate, tailwind_difficulty, tailwind_hashrate)
        forward = (future / current - 1.0) * 100.0
        rows.append({
            "recorded_at_ms": record.get("recorded_at_ms"), "state": state,
            "forward_return_pct": forward, "forward_abs_return_pct": abs(forward),
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


def summarize(records, horizon_records, stress_difficulty, stress_hashrate,
              tailwind_difficulty, tailwind_hashrate, min_observations):
    rows = observations(records, horizon_records, stress_difficulty, stress_hashrate,
                        tailwind_difficulty, tailwind_hashrate)
    states = ("miner_stress_context", "miner_tailwind_context", "ordinary_mining_context")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    stress = by_state["miner_stress_context"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_mining_context"]["mean_absolute_forward_return_pct"]
    edge_bps = ((stress - ordinary) * 100.0
                if stress is not None and ordinary is not None else None)
    stress_count = by_state["miner_stress_context"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "miner_stress_vs_ordinary_absolute_move_edge_bps": edge_bps,
        "verdict": ("mining_pressure_response_reported"
                    if stress_count >= min_observations
                    else "observe_only_insufficient_miner_stress_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--stress-difficulty-pct", type=float, default=-3.0)
    parser.add_argument("--stress-hashrate-pct", type=float, default=-3.0)
    parser.add_argument("--tailwind-difficulty-pct", type=float, default=3.0)
    parser.add_argument("--tailwind-hashrate-pct", type=float, default=3.0)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.min_observations <= 0
            or args.stress_difficulty_pct > args.tailwind_difficulty_pct
            or args.stress_hashrate_pct > args.tailwind_hashrate_pct):
        parser.error("invalid horizon, observation count or mining thresholds")
    records, invalid_lines = load_records(args.input)
    summary = summarize(records, args.horizon_records, args.stress_difficulty_pct,
                        args.stress_hashrate_pct, args.tailwind_difficulty_pct,
                        args.tailwind_hashrate_pct, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_onchain_mining_pressure_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {
            "horizon_records": args.horizon_records,
            "stress_difficulty_pct": args.stress_difficulty_pct,
            "stress_hashrate_pct": args.stress_hashrate_pct,
            "tailwind_difficulty_pct": args.tailwind_difficulty_pct,
            "tailwind_hashrate_pct": args.tailwind_hashrate_pct,
            "min_observations": args.min_observations,
        },
        "summary": summary,
        "limitations": [
            "hashrate and difficulty are provider estimates, not miner identity or profitability",
            "the replay tests association with later BTC movement, not capitulation causality or direction",
            "no miner cash-flow, reserve, transaction, wallet, sizing, fills or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
