#!/usr/bin/env python3
"""Replay directional BTC response after normalized USDC/USDT quote states."""

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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def usdc_usdt_deviation_bps(observation):
    for quote in observation.get("stablecoin_quotes", []):
        base, counter = str(quote.get("base", "")).upper(), str(quote.get("counter", "")).upper()
        mid = number(quote.get("mid"))
        if mid is None or mid <= 0:
            continue
        if base == "USDC" and counter == "USDT":
            return (mid - 1.0) * 10_000.0
        if base == "USDT" and counter == "USDC":
            return (1.0 / mid - 1.0) * 10_000.0
    return None


def state_for(deviation_bps, threshold_bps):
    if deviation_bps is None:
        return "missing_usdc_usdt_quote"
    if deviation_bps <= -threshold_bps:
        return "usdc_discount"
    if deviation_bps >= threshold_bps:
        return "usdc_premium"
    return "balanced_usdc_usdt"


def bucket_stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    aligned = [row["aligned_return_bps"] for row in rows
               if row["aligned_return_bps"] is not None]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns)
        if returns else None,
        "mean_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "positive_aligned_fraction": (sum(value > 0 for value in aligned) / len(aligned))
        if aligned else None,
    }


def summarize_records(records, horizon_snapshots, threshold_bps, min_observations):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    observations = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_snapshots
        if future_index >= len(ordered):
            continue
        current = number((((record.get("observation") or {}).get("risk_asset") or {}).get("mid")))
        future = number((((ordered[future_index].get("observation") or {}).get("risk_asset") or {}).get("mid")))
        if current is None or future is None or current <= 0 or future <= 0:
            continue
        deviation = usdc_usdt_deviation_bps(record.get("observation") or {})
        state = state_for(deviation, threshold_bps)
        forward = (future / current - 1.0) * 100.0
        expected_sign = 1 if state == "usdc_discount" else -1 if state == "usdc_premium" else 0
        observations.append({
            "recorded_at_ms": record.get("recorded_at_ms"), "state": state,
            "usdc_usdt_deviation_bps": deviation, "forward_return_pct": forward,
            "aligned_return_bps": expected_sign * forward * 100.0 if expected_sign else None,
        })
    states = ("usdc_discount", "usdc_premium", "balanced_usdc_usdt", "missing_usdc_usdt_quote")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    directional = sum(by_state[state]["observations"] for state in ("usdc_discount", "usdc_premium"))
    return {
        "by_state": by_state, "aligned_forward_windows": len(observations),
        "directional_usdc_windows": directional,
        "verdict": ("stablecoin_rotation_response_reported" if directional >= min_observations
                    else "observe_only_insufficient_directional_usdc_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-snapshots", type=int, default=3)
    parser.add_argument("--threshold-bps", type=float, default=5.0)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.horizon_snapshots <= 0 or args.threshold_bps < 0 or args.min_observations <= 0:
        parser.error("horizon and minimum observations must be positive; threshold cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_stablecoin_rotation_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"horizon_snapshots": args.horizon_snapshots, "threshold_bps": args.threshold_bps,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.horizon_snapshots, args.threshold_bps,
                                      args.min_observations),
        "limitations": [
            "USDC/USDT inversion is a quote normalization, not proof of capital flow or causality",
            "one venue's stablecoin pair is not a complete global stablecoin market",
            "directional alignment is descriptive and does not model fees, conversion, redemption or execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
