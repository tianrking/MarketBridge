#!/usr/bin/env python3
"""Replay BTC responses after recorded Coinbase premium states."""

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
    value = number((((record.get("observation") or {}).get("reference_quote") or {}).get("price")))
    return value if value is not None and value > 0 else None


def state_of(record):
    return (record.get("observation") or {}).get("state",
            "observe_only_missing_premium_alignment")


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
            "recorded_at_ms": record.get("recorded_at_ms"), "state": state_of(record),
            "forward_return_pct": forward, "forward_abs_return_pct": abs(forward),
            "horizon_recorded_at_ms": ordered[future_index].get("recorded_at_ms"),
        })
    return rows


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_absolute_forward_return_pct": statistics.median(absolute) if absolute else None,
    }


def summarize_records(records, horizon_records, min_observations):
    rows = observations(records, horizon_records)
    states = ("coinbase_premium", "coinbase_discount", "ordinary_coinbase_reference_spread")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    premium = by_state["coinbase_premium"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_coinbase_reference_spread"]["mean_absolute_forward_return_pct"]
    edge_bps = ((premium - ordinary) * 100.0
                if premium is not None and ordinary is not None else None)
    qualifying = sum(by_state[state]["observations"] for state in states)
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "premium_minus_ordinary_absolute_move_edge_bps": edge_bps,
        "verdict": ("coinbase_premium_response_reported" if qualifying >= min_observations
                    else "observe_only_insufficient_coinbase_premium_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_coinbase_premium_response_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_records, args.min_observations),
        "limitations": [
            "Coinbase USD and reference USDT quotes can include a stablecoin or FX basis",
            "a cross-venue quote spread is not a pure US spot-flow or arbitrage measure",
            "the replay uses fixed-record association with no fee, transfer, inventory, fill or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
