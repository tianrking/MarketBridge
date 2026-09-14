#!/usr/bin/env python3
"""Replay BTC response after recorded option IV-minus-RV regimes."""

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
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def vrp_value(record):
    return number((((record.get("observation") or {}).get("vrp") or {})
                  .get("iv_minus_rv_iv_points")))


def state(value, threshold):
    if value is None:
        return "observe_only_missing_iv_or_rv"
    if value >= threshold:
        return "implied_volatility_premium"
    if value <= -threshold:
        return "realized_volatility_above_implied"
    return "implied_and_realized_vol_aligned"


def spot_close(record):
    value = number((((record.get("observation") or {}).get("realized_volatility") or {})
                    .get("spot_close")))
    return value if value is not None and value > 0 else None


def stats(rows):
    returns = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_iv_minus_rv_iv_points": (
            statistics.median(row["iv_minus_rv_iv_points"] for row in rows)
            if rows else None
        ),
        "expiry_observations": len({row["expiry"] for row in rows if row["expiry"]}),
    }


def summarize_records(records, threshold, horizon_records, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    buckets = {
        "implied_volatility_premium": [],
        "realized_volatility_above_implied": [],
        "implied_and_realized_vol_aligned": [],
        "observe_only_missing_iv_or_rv": [],
    }
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = spot_close(record), spot_close(ordered[future_index])
        value = vrp_value(record)
        bucket = state(value, threshold)
        if current is None or future is None:
            continue
        buckets[bucket].append({
            "forward_return_pct": (future / current - 1.0) * 100.0,
            "forward_abs_return_pct": abs((future / current - 1.0) * 100.0),
            "iv_minus_rv_iv_points": value,
            "expiry": ((record.get("observation") or {}).get("target_expiry") or {})
            .get("expiry_time"),
        })
    by_state = {key: stats(rows) for key, rows in buckets.items()}
    premium_abs = by_state["implied_volatility_premium"]["mean_absolute_forward_return_pct"]
    aligned_abs = by_state["implied_and_realized_vol_aligned"]["mean_absolute_forward_return_pct"]
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": sum(len(rows) for rows in buckets.values()),
        "by_state": by_state,
        "premium_absolute_move_edge_bps": (
            (premium_abs - aligned_abs) * 100.0
            if premium_abs is not None and aligned_abs is not None else None
        ),
        "verdict": (
            "options_vrp_response_reported"
            if any(by_state[key]["observations"] >= min_observations
                   for key in ("implied_volatility_premium",
                               "realized_volatility_above_implied"))
            else "observe_only_insufficient_vrp_regimes"
        ),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if args.vrp_threshold < 0 or args.horizon_records <= 0 or args.min_observations <= 0:
        parser.error("vrp-threshold cannot be negative; horizon and min-observations must be positive")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_options_vrp_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"vrp_threshold": args.vrp_threshold,
                    "horizon_records": args.horizon_records,
                    "min_observations": args.min_observations},
        "summary": summarize_records(records, args.vrp_threshold,
                                      args.horizon_records, args.min_observations),
        "limitations": [
            "IV and RV use different horizons and do not form an option PnL calculation",
            "the recorder's selected expiry can roll between snapshots and is reported as metadata",
            "spot_close is a bounded candle observation; missing values are excluded",
            "no option position, delta hedge, funding, fee, margin or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
