#!/usr/bin/env python3
"""Replay BTC response distributions by aggregate MarketBridge regime."""

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


def price(record):
    value = ((record.get("observation") or {}).get("price") or {}).get("price")
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0 else None


def regime(record):
    return ((record.get("observation") or {}).get("regime") or "observe_only_unknown_regime")


def stats(rows, paper_cost_bps):
    returns = [row["forward_return_pct"] for row in rows]
    adjusted = [value - paper_cost_bps / 100.0 for value in returns]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "median_forward_return_pct": statistics.median(returns) if returns else None,
        "mean_absolute_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
        "downside_fraction": sum(value < 0 for value in returns) / len(returns) if returns else None,
        "mean_after_cost_return_pct": statistics.mean(adjusted) if adjusted else None,
    }


def summarize_records(records, horizon_records, min_observations, paper_cost_bps):
    ordered = sorted(records, key=lambda row: row.get("recorded_at_ms", 0))
    buckets = {}
    gaps, aligned = [], 0
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        baseline = price(record)
        future = price(ordered[future_index]) if future_index < len(ordered) else None
        if baseline is None or future is None:
            continue
        aligned += 1
        buckets.setdefault(regime(record), []).append({
            "forward_return_pct": (future / baseline - 1.0) * 100.0,
        })
        gaps.append(ordered[future_index].get("recorded_at_ms", 0)
                    - record.get("recorded_at_ms", 0))
    summaries = {key: stats(rows, paper_cost_bps) for key, rows in sorted(buckets.items())}
    return {"snapshots": len(ordered), "aligned_forward_windows": aligned,
            "median_record_gap_ms": statistics.median(gaps) if gaps else None,
            "by_regime": summaries,
            "verdict": "market_regime_response_distribution_reported"
            if aligned >= min_observations else "observe_only_insufficient_regime_observations",
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--horizon-records", type=int, default=7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    args = parser.parse_args()
    if args.horizon_records <= 0 or args.min_observations <= 0 or args.paper_cost_bps < 0:
        parser.error("horizon and minimum observations must be positive; cost cannot be negative")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({"strategy": "crypto_market_regime_replay", "input": str(args.input),
                      "invalid_lines": invalid_lines,
                      "filters": {"horizon_records": args.horizon_records,
                                  "min_observations": args.min_observations,
                                  "paper_cost_bps": args.paper_cost_bps},
                      "summary": summarize_records(records, args.horizon_records,
                                                    args.min_observations, args.paper_cost_bps),
                      "limitations": [
                          "market-regime rows are a current aggregate feature snapshot, not point-in-time historical factors",
                          "record-count horizons do not imply exact elapsed time, fills or causality",
                          "paper cost is a sensitivity input, not fees, funding, slippage or execution",
                          "regime labels are context and do not select, size or execute a strategy",
                      ], "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
