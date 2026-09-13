#!/usr/bin/env python3
"""Replay whether unsigned near-spot gamma concentration precedes BTC movement."""

import argparse
import json
import statistics
from pathlib import Path

from crypto_options_gamma_monitor import classify_gamma


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


def price(record):
    return number(((record.get("observation") or {}).get("price") or {}).get("price"))


def stats(values):
    return {"observations": len(values),
            "mean_forward_return_pct": statistics.mean(values) if values else None,
            "median_forward_return_pct": statistics.median(values) if values else None,
            "mean_absolute_return_pct": statistics.mean(abs(value) for value in values) if values else None}


def summarize_records(records, horizon_records, min_near_share, min_concentration,
                      min_observations):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {"near_spot_gamma_concentration": [], "other_gamma_state": []}
    aligned = 0
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline, future = price(record), price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or baseline <= 0 or future is None or future <= 0:
            continue
        target = ((record.get("observation") or {}).get("gamma") or {}).get("target_expiry") or {}
        state = classify_gamma(target, min_near_share, min_concentration)
        buckets["near_spot_gamma_concentration" if state == "near_spot_gamma_concentration"
                else "other_gamma_state"].append((future / baseline - 1.0) * 100.0)
        aligned += 1
    concentrated = buckets["near_spot_gamma_concentration"]
    return {"snapshots": len(ordered), "aligned_forward_windows": aligned,
            "by_state": {key: stats(values) for key, values in buckets.items()},
            "verdict": ("gamma_concentration_response_reported"
                        if len(concentrated) >= min_observations
                        else "observe_only_insufficient_gamma_observations"),
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-near-share", type=float, default=0.50)
    parser.add_argument("--min-concentration", type=float, default=0.10)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or not 0 <= args.min_near_share <= 1
            or not 0 <= args.min_concentration <= 1 or args.min_observations <= 0):
        parser.error("horizon and observations must be positive; thresholds must be in [0, 1]")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_options_gamma_response_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_near_share": args.min_near_share,
                                  "min_concentration": args.min_concentration,
                                  "min_observations": args.min_observations},
                      "summary": summarize_records(records, args.horizon_records,
                                                    args.min_near_share, args.min_concentration,
                                                    args.min_observations),
                      "limitations": [
                          "unsigned gamma concentration does not reveal dealer gamma sign or causality",
                          "absolute movement is descriptive and is not option PnL, hedge demand or a directional signal",
                          "fixed record horizons are not exact elapsed time; missing quotes are excluded",
                      ], "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
