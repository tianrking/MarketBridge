#!/usr/bin/env python3
"""Replay persistence of recorded Jupiter route-impact states."""

import argparse
import json
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


def longest_run(states, target):
    best = current = 0
    for state in states:
        current = current + 1 if state == target else 0
        best = max(best, current)
    return best


def summarize_records(records, min_run):
    groups = {}
    for record in sorted(records, key=lambda item: item.get("recorded_at_ms", 0)):
        for route in (record.get("observation") or {}).get("routes", []):
            key = (str(route.get("source", "")).lower(), str(route.get("symbol", "")).upper(),
                   route.get("input_amount_atomic"))
            if key[0] and key[1] and isinstance(key[2], int):
                groups.setdefault(key, []).append(route)
    by_route = {}
    for (source, symbol, amount), routes in sorted(groups.items()):
        states = [route.get("state", "observe_only_missing_route") for route in routes]
        high = "high_route_impact"
        by_route[f"{source}:{symbol}:{amount}"] = {
            "observations": len(routes),
            "high_impact_observations": states.count(high),
            "high_impact_share": states.count(high) / len(states) if states else None,
            "longest_high_impact_run": longest_run(states, high),
            "state_counts": {state: states.count(state) for state in sorted(set(states))},
            "verdict": "persistent_jupiter_route_impact_candidate"
            if longest_run(states, high) >= min_run else "observe_only_no_persistent_route_impact",
        }
    return {"routes": len(by_route), "snapshots": len(records), "by_route": by_route,
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--min-run", type=int, default=3)
    args = parser.parse_args()
    if args.min_run <= 0:
        parser.error("min-run must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_jupiter_route_impact_replay",
                      "input": str(args.input), "invalid_lines": invalid_lines,
                      "filters": {"min_run": args.min_run},
                      "summary": summarize_records(records, args.min_run),
                      "limitations": [
                          "route impact is a public router snapshot, not realized slippage or fill quality",
                          "configured quote-size ladders do not establish complete pool depth",
                          "no gas, MEV, wallet, transaction or execution model is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
