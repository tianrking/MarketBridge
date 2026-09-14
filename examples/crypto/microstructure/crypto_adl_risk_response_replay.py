#!/usr/bin/env python3
"""Replay BTC responses after Binance ADL-risk context states."""

import argparse
import json
import statistics
from pathlib import Path

from crypto_adl_risk_monitor import classify_risk


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
    rows = (record.get("observation") or {}).get("adl_rows", [])
    states = [classify_risk(row.get("adl_risk")) for row in rows if isinstance(row, dict)]
    if "adl_risk_high" in states:
        return "adl_risk_high_context"
    if "adl_risk_medium" in states:
        return "adl_risk_medium_context"
    if "adl_risk_low" in states:
        return "adl_risk_low_context"
    return "observe_only_unknown_adl_risk"


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
    states = ("adl_risk_high_context", "adl_risk_medium_context", "adl_risk_low_context")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    high = by_state["adl_risk_high_context"]["mean_absolute_forward_return_pct"]
    low = by_state["adl_risk_low_context"]["mean_absolute_forward_return_pct"]
    edge_bps = ((high - low) * 100.0 if high is not None and low is not None else None)
    qualifying = sum(by_state[state]["observations"] for state in states)
    return {
        "by_state": by_state, "aligned_forward_windows": len(rows),
        "high_vs_low_absolute_move_edge_bps": edge_bps,
        "verdict": ("adl_risk_response_reported" if qualifying >= min_observations
                    else "observe_only_insufficient_adl_risk_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=2)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, args.horizon_records, args.min_observations)
    print(json.dumps({
        "strategy": "crypto_adl_risk_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summary,
        "limitations": [
            "ADL risk is a provider rating updated on a provider schedule, not a realized ADL event",
            "the public snapshot does not expose private account risk, insurance-fund flows or a causal price mechanism",
            "high/medium/low states are context buckets; no direction, order, wallet, sizing or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
