#!/usr/bin/env python3
"""Replay BTC responses after high, low and ordinary mempool pressure."""

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


def classify(data, high_fee, low_fee, high_vsize, low_vsize):
    if not isinstance(data, dict):
        return "observe_only_missing_mempool_metrics"
    fees = data.get("fee_rates_sat_vb") or {}
    fastest = number(fees.get("fastest"))
    vsize = number(data.get("mempool_vsize_mb"))
    if fastest is None or vsize is None:
        return "observe_only_missing_mempool_metrics"
    if fastest >= high_fee or vsize >= high_vsize:
        return "high_fee_pressure"
    if fastest <= low_fee and vsize <= low_vsize:
        return "low_fee_pressure"
    return "ordinary_fee_pressure"


def observations(records, horizon_records, high_fee, low_fee, high_vsize, low_vsize):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    rows = []
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        current, future = price_of(record), price_of(ordered[future_index])
        if current is None or future is None:
            continue
        state = classify((record.get("observation") or {}).get("mempool"), high_fee,
                         low_fee, high_vsize, low_vsize)
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


def summarize(records, horizon_records, high_fee, low_fee, high_vsize, low_vsize,
              min_observations):
    rows = observations(records, horizon_records, high_fee, low_fee, high_vsize, low_vsize)
    states = ("high_fee_pressure", "low_fee_pressure", "ordinary_fee_pressure")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    high = by_state["high_fee_pressure"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_fee_pressure"]["mean_absolute_forward_return_pct"]
    edge_bps = ((high - ordinary) * 100.0
                if high is not None and ordinary is not None else None)
    high_count = by_state["high_fee_pressure"]["observations"]
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "high_vs_ordinary_absolute_move_edge_bps": edge_bps,
        "verdict": ("mempool_pressure_response_reported"
                    if high_count >= min_observations
                    else "observe_only_insufficient_high_pressure_observations"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=12)
    parser.add_argument("--high-fee-sat-vb", type=float, default=20.0)
    parser.add_argument("--low-fee-sat-vb", type=float, default=3.0)
    parser.add_argument("--high-vsize-mb", type=float, default=150.0)
    parser.add_argument("--low-vsize-mb", type=float, default=25.0)
    parser.add_argument("--min-observations", type=int, default=3)
    args = parser.parse_args()
    if (args.horizon_records <= 0 or args.min_observations <= 0
            or args.high_fee_sat_vb < args.low_fee_sat_vb or args.low_fee_sat_vb < 0
            or args.high_vsize_mb < args.low_vsize_mb or args.low_vsize_mb < 0):
        parser.error("invalid horizon, observation count or pressure thresholds")
    records, invalid_lines = load_records(args.input)
    summary = summarize(records, args.horizon_records, args.high_fee_sat_vb,
                        args.low_fee_sat_vb, args.high_vsize_mb, args.low_vsize_mb,
                        args.min_observations)
    print(json.dumps({
        "strategy": "crypto_onchain_mempool_pressure_replay",
        "input": str(args.input), "invalid_lines": invalid_lines,
        "filters": {
            "horizon_records": args.horizon_records,
            "high_fee_sat_vb": args.high_fee_sat_vb,
            "low_fee_sat_vb": args.low_fee_sat_vb,
            "high_vsize_mb": args.high_vsize_mb,
            "low_vsize_mb": args.low_vsize_mb,
            "min_observations": args.min_observations,
        },
        "summary": summary,
        "limitations": [
            "mempool state is provider- and node-dependent, not a complete network-wide ledger",
            "fee estimates are guidance and do not guarantee confirmation timing",
            "the replay tests association with later BTC movement, not causality or direction",
            "no transaction broadcast, wallet signing, fee choice, sizing, fills or trading path",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
