#!/usr/bin/env python3
"""Replay BTC responses after provider funding-band proximity states."""

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


def state_of(record):
    rows = (record.get("observation") or {}).get("funding_rows", [])
    states = {row.get("state") for row in rows if isinstance(row, dict)}
    if "near_upper_funding_cap" in states:
        return "near_upper_funding_cap"
    if "near_lower_funding_floor" in states:
        return "near_lower_funding_floor"
    if "within_provider_funding_band" in states:
        return "within_provider_funding_band"
    return "observe_only_missing_provider_band"


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
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_absolute_forward_return_pct": statistics.median(absolute) if absolute else None,
    }


def summarize_records(records, horizon_records, min_observations):
    rows = observations(records, horizon_records)
    states = ("near_upper_funding_cap", "near_lower_funding_floor", "within_provider_funding_band")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    boundary = by_state["near_upper_funding_cap"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["within_provider_funding_band"]["mean_absolute_forward_return_pct"]
    edge_bps = ((boundary - ordinary) * 100.0
                if boundary is not None and ordinary is not None else None)
    qualifying = sum(by_state[state]["observations"] for state in states)
    return {
        "by_state": by_state, "aligned_forward_windows": len(rows),
        "upper_cap_vs_ordinary_absolute_move_edge_bps": edge_bps,
        "verdict": ("funding_band_response_reported" if qualifying >= min_observations
                    else "observe_only_insufficient_funding_band_observations"),
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
    summary = summarize_records(records, args.horizon_records, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_funding_band_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summary,
        "limitations": [
            "cap/floor and interval are provider parameters that can change over time",
            "proximity does not forecast price, liquidation or realized funding income",
            "the replay tests fixed-record association; no hedge, borrow, fee, fill or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
