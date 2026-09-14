#!/usr/bin/env python3
"""Replay BTC response after recorded put/call open-interest states."""

import argparse
import json
import statistics
from pathlib import Path


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


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


def aligned_observations(records, horizon_records):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            continue
        state = ((record.get("observation") or {}).get("oi") or {}).get(
            "state", "observe_only_missing_put_call_oi")
        baseline = number(((record.get("observation") or {}).get("price") or {}).get("price"))
        future = number(((ordered[future_index].get("observation") or {}).get("price") or {}).get("price"))
        if baseline is None or future is None or baseline <= 0:
            continue
        observations.append({"state": state, "forward_return_pct": (future / baseline - 1.0) * 100.0})
    return observations


def summarize(observations, min_observations):
    states = ("defensive_put_oi", "call_dominant_oi", "balanced_oi",
              "observe_only_missing_put_call_oi")
    by_state = {state: [row["forward_return_pct"] for row in observations if row["state"] == state]
                for state in states}
    stats = {}
    for state, values in by_state.items():
        stats[state] = {
            "observations": len(values),
            "mean_forward_return_pct": statistics.mean(values) if values else None,
            "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in values) if values else None,
        }
    return {"by_state": stats,
            "aligned_windows": len(observations),
            "verdict": "options_put_call_oi_response_reported" if len(observations) >= min_observations
            else "observe_only_insufficient_aligned_oi_windows",
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("horizon-records and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    observations = aligned_observations(records, args.horizon_records)
    print(json.dumps({"strategy": "crypto_options_put_call_oi_response_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_observations": args.min_observations},
                      "summary": summarize(observations, args.min_observations),
                      "limitations": [
                          "record-count horizon is not elapsed-time normalization",
                          "open-interest snapshots roll across expiries and do not identify dealer sign",
                          "response comparison is descriptive; no option PnL, hedge, cost or execution model",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
