#!/usr/bin/env python3
"""Observe read-only Orca Whirlpool turnover and CLMM risk context.

The falsifiable hypothesis is narrow: a Whirlpool with high 24-hour volume
relative to its top-pool TVL and an Orca-reported warning or adaptive-fee flag
may persist as a distinct execution-risk context. This consumes only
MarketBridge ``defi_native_state`` metrics; it does not manage positions,
construct swaps, sign wallets, or estimate LP PnL.
"""

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_microstructure_monitor import fetch  # noqa: E402


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def summarize_signals(signals):
    grouped = {}
    for row in signals:
        if not isinstance(row, dict):
            continue
        source = str(row.get("source", "")).lower()
        symbol = str(row.get("symbol", "")).upper()
        metric = str(row.get("metric", "")).lower()
        value = number(row.get("value"))
        if source and symbol and metric and value is not None and math.isfinite(value):
            grouped.setdefault((source, symbol), {})[metric] = value
    return grouped


def classify_pool(metrics, min_turnover_ratio):
    turnover = metrics.get("pool_top_turnover_24h_ratio")
    warning = metrics.get("pool_top_has_warning")
    adaptive = metrics.get("pool_top_adaptive_fee_enabled")
    if turnover is None or warning is None or adaptive is None:
        return "observe_only_missing_orca_metrics"
    if metrics.get("pool_search_has_next_page", 0.0) >= 0.5:
        return "observe_only_partial_orca_page"
    if turnover >= min_turnover_ratio and (warning >= 0.5 or adaptive >= 0.5):
        return "warning_or_adaptive_fee_pressure"
    if turnover >= min_turnover_ratio:
        return "high_turnover_whirlpool"
    return "ordinary_whirlpool_state"


def observe_pools(base_url, symbols, min_turnover_ratio, timeout):
    payload = fetch(base_url, "/v1/external/signals", {
        "categories": "defi_native_state",
        "sources": "orca",
        "symbols": symbols or None,
    }, timeout)
    pools = []
    for (source, symbol), metrics in sorted(summarize_signals(payload.get("signals", [])).items()):
        pools.append({
            "source": source,
            "symbol": symbol,
            "pool_count": metrics.get("pool_count"),
            "has_next_page": metrics.get("pool_search_has_next_page"),
            "tvl_usdc_total": metrics.get("pool_tvl_usdc_total"),
            "volume_24h_usdc_total": metrics.get("pool_volume_24h_usdc_total"),
            "fees_24h_usdc_total": metrics.get("pool_fees_24h_usdc_total"),
            "top_tvl_usdc": metrics.get("pool_top_tvl_usdc"),
            "top_tvl_share": metrics.get("pool_top_tvl_share"),
            "top_volume_24h_usdc": metrics.get("pool_top_volume_24h_usdc"),
            "top_yield_over_tvl": metrics.get("pool_top_yield_over_tvl"),
            "top_fee_rate_raw": metrics.get("pool_top_fee_rate_raw"),
            "top_price": metrics.get("pool_top_price"),
            "top_turnover_24h_ratio": metrics.get("pool_top_turnover_24h_ratio"),
            "top_has_warning": metrics.get("pool_top_has_warning"),
            "top_adaptive_fee_enabled": metrics.get("pool_top_adaptive_fee_enabled"),
            "state": classify_pool(metrics, min_turnover_ratio),
        })
    return {
        "pools": pools,
        "pool_count": len(pools),
        "upstream_errors": payload.get("errors", []),
        "evidence": ["orca_whirlpool_state_available" if pools else "missing_orca_whirlpool_state"],
        "limitations": [
            "Orca REST values are provider snapshots and cursor pages, not a complete swap ledger",
            "warning/adaptive-fee flags are context and do not identify LP PnL or realized slippage",
            "top-pool turnover does not model active tick range, gas, MEV, fill probability or routing",
            "no position, transaction, wallet, signing or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None, help="comma-separated Orca symbols")
    parser.add_argument("--min-turnover-ratio", type=float, default=1.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.min_turnover_ratio < 0 or args.timeout <= 0:
        parser.error("turnover threshold must be non-negative and timeout positive")
    print(json.dumps({
        "strategy": "crypto_orca_whirlpool_monitor",
        "filters": {"symbols": args.symbols, "min_turnover_ratio": args.min_turnover_ratio},
        **observe_pools(args.base_url, args.symbols, args.min_turnover_ratio, args.timeout),
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
