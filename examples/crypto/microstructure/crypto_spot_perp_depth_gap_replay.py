#!/usr/bin/env python3
"""Replay persistence of recorded spot/perp depth asymmetry."""

import argparse
import json
from pathlib import Path


def load_records(path):
    records = []
    invalid_lines = 0
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


def depth_metrics(observation):
    spot = (observation.get("spot") or {}).get("metrics") or {}
    perp = (observation.get("perp") or {}).get("metrics") or {}
    spot_depth = (spot.get("bid_depth_notional") or 0.0) + (spot.get("ask_depth_notional") or 0.0)
    perp_depth = (perp.get("bid_depth_notional") or 0.0) + (perp.get("ask_depth_notional") or 0.0)
    spot_impacts = [spot.get("buy_impact_bps"), spot.get("sell_impact_bps")]
    perp_impacts = [perp.get("buy_impact_bps"), perp.get("sell_impact_bps")]
    spot_impacts = [value for value in spot_impacts if isinstance(value, (int, float))]
    perp_impacts = [value for value in perp_impacts if isinstance(value, (int, float))]
    if spot_depth <= 0 or perp_depth <= 0 or not spot_impacts or not perp_impacts:
        return None
    return {
        "depth_ratio": perp_depth / spot_depth,
        "impact_improvement_bps": max(spot_impacts) - max(perp_impacts),
        "basis_bps": observation.get("basis_bps"),
        "state": observation.get("state"),
    }


def longest_run(values, target):
    best = current = 0
    for value in values:
        current = current + 1 if value == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_depth_ratio, min_impact_improvement_bps, min_run):
    metrics = [metric for record in records
               if (metric := depth_metrics(record["observation"])) is not None]
    advantages = [
        metric["depth_ratio"] >= min_depth_ratio
        and metric["impact_improvement_bps"] >= min_impact_improvement_bps
        for metric in metrics
    ]
    advantage_count = sum(advantages)
    persistent = (len(metrics) >= min_run and advantage_count / len(metrics) >= 0.5
                  and longest_run(advantages, True) >= min_run)
    return {
        "snapshots": len(records),
        "valid_depth_snapshots": len(metrics),
        "perp_advantage_snapshots": advantage_count,
        "perp_advantage_fraction": advantage_count / len(metrics) if metrics else None,
        "longest_perp_advantage_run": longest_run(advantages, True),
        "mean_depth_ratio": (
            sum(metric["depth_ratio"] for metric in metrics) / len(metrics)
            if metrics else None
        ),
        "mean_impact_improvement_bps": (
            sum(metric["impact_improvement_bps"] for metric in metrics) / len(metrics)
            if metrics else None
        ),
        "verdict": "persistent_perp_depth_advantage" if persistent else "observe_only",
        "evidence": [
            "valid_spot_perp_depth_snapshots" if metrics else "missing_target_size_depth_snapshots",
            "perp_depth_advantage_persistent" if persistent
            else "perp_depth_advantage_not_persistent_or_insufficient_history",
        ],
        "metrics": metrics,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-depth-ratio", type=float, default=2.0)
    parser.add_argument("--min-impact-improvement-bps", type=float, default=5.0)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_depth_ratio < 1.0 or args.min_impact_improvement_bps < 0 or args.min_run <= 0:
        parser.error("invalid depth, impact or run thresholds")
    records, invalid_lines = load_records(args.input)
    summary = summarize_records(records, args.min_depth_ratio,
                                args.min_impact_improvement_bps, args.min_run)
    print(json.dumps({
        "strategy": "crypto_spot_perp_depth_gap_replay",
        "input": str(args.input),
        "invalid_lines": invalid_lines,
        "parameters": {
            "min_depth_ratio": args.min_depth_ratio,
            "min_impact_improvement_bps": args.min_impact_improvement_bps,
            "min_run": args.min_run,
        },
        "summary": summary,
        "limitations": [
            "snapshots are descriptive and may not be synchronized or executable fills",
            "persistence does not establish a hedge, basis convergence or routing edge",
            "fees, latency, queue position, capacity and inventory are excluded",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
