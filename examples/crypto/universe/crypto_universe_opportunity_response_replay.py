#!/usr/bin/env python3
"""Replay top-k universe candidate-basket response relative to BTC."""

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


def candidate_symbols(record, top_k):
    rows = (record.get("observation") or {}).get("candidates", [])
    return tuple(str(row.get("symbol", "")).upper() for row in rows[:top_k] if row.get("symbol"))


def price_map(record):
    result = {}
    for symbol, value in ((record.get("observation") or {}).get("response_prices") or {}).items():
        raw = value.get("price") if isinstance(value, dict) else value
        price = number(raw)
        if price is not None and price > 0:
            result[str(symbol).upper()] = price
    return result


def stats(rows):
    valid = [row for row in rows if "relative_return_pct" in row]
    relative = [row["relative_return_pct"] for row in valid]
    basket = [row["basket_return_pct"] for row in valid]
    benchmark = [row["benchmark_return_pct"] for row in valid]
    return {
        "snapshots": len(rows),
        "observations": len(valid),
        "mean_candidate_basket_return_pct": statistics.mean(basket) if basket else None,
        "mean_benchmark_return_pct": statistics.mean(benchmark) if benchmark else None,
        "mean_candidate_minus_benchmark_pct": statistics.mean(relative) if relative else None,
        "relative_positive_fraction": (sum(value > 0 for value in relative) / len(relative)
                                        if relative else None),
        "median_assets_used": statistics.median(
            row["assets_used"] for row in rows if "assets_used" in row
        ) if any("assets_used" in row for row in rows) else None,
    }


def summarize_records(records, top_k, horizon_records, min_assets, min_observations):
    ordered = sorted(records, key=lambda item: item.get("recorded_at_ms", 0))
    buckets = {"candidate_set_available": [], "candidate_set_missing_prices": [], "no_candidates": []}
    for index, record in enumerate(ordered):
        future_index = index + horizon_records
        if future_index >= len(ordered):
            break
        candidates = candidate_symbols(record, top_k)
        if not candidates:
            buckets["no_candidates"].append({})
            continue
        current_prices = price_map(record)
        future_prices = price_map(ordered[future_index])
        benchmark = str((record.get("observation") or {}).get("benchmark_symbol", "BTCUSDT")).upper()
        current_benchmark, future_benchmark = current_prices.get(benchmark), future_prices.get(benchmark)
        usable = [symbol for symbol in candidates
                   if symbol in current_prices and symbol in future_prices]
        if (len(usable) < min_assets or current_benchmark is None or future_benchmark is None):
            buckets["candidate_set_missing_prices"].append({"assets_used": len(usable)})
            continue
        basket_return = statistics.mean(
            (future_prices[symbol] / current_prices[symbol] - 1.0) * 100.0
            for symbol in usable
        )
        benchmark_return = (future_benchmark / current_benchmark - 1.0) * 100.0
        buckets["candidate_set_available"].append({
            "basket_return_pct": basket_return, "benchmark_return_pct": benchmark_return,
            "relative_return_pct": basket_return - benchmark_return,
            "assets_used": len(usable),
        })
    by_state = {key: stats(rows) for key, rows in buckets.items()}
    return {
        "snapshots": len(ordered),
        "aligned_forward_windows": len(buckets["candidate_set_available"]),
        "by_state": by_state,
        "verdict": ("universe_candidate_response_reported"
                    if by_state["candidate_set_available"]["observations"] >= min_observations
                    else "observe_only_insufficient_candidate_responses"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--top-k", type=int, default=3)
    parser.add_argument("--horizon-records", type=int, default=3)
    parser.add_argument("--min-assets", type=int, default=1)
    parser.add_argument("--min-observations", type=int, default=5)
    args = parser.parse_args()
    if (args.top_k <= 0 or args.horizon_records <= 0 or args.min_assets <= 0
            or args.min_assets > args.top_k or args.min_observations <= 0):
        parser.error("invalid top-k, horizon, minimum assets or observations")
    records, invalid_lines = load_records(args.input)
    print(json.dumps({
        "strategy": "crypto_universe_opportunity_response_replay", "input": str(args.input),
        "invalid_lines": invalid_lines,
        "filters": {"top_k": args.top_k, "horizon_records": args.horizon_records,
                    "min_assets": args.min_assets, "min_observations": args.min_observations},
        "summary": summarize_records(records, args.top_k, args.horizon_records,
                                      args.min_assets, args.min_observations),
        "limitations": [
            "candidate selection is point-in-time but the equal-weight basket is a paper index",
            "future prices are required for the same selected symbols; missing rows remain explicit",
            "benchmark-relative movement is not allocation, rebalancing or risk-adjusted PnL",
            "no fees, funding income, borrow, liquidity impact, capacity, hedge or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
