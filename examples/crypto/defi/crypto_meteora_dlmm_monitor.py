#!/usr/bin/env python3
"""Observe read-only Meteora DLMM fee/turnover and pool-risk context.

Hypothesis: unusually high 24-hour turnover together with fee/TVL or dynamic
fee pressure may identify a persistent liquidity-risk regime. This is a
research observer only; it never routes swaps, signs wallets, or submits orders.
"""

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_microstructure_monitor import fetch  # noqa: E402


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value)) else None


def summarize_signals(signals):
    grouped = {}
    for row in signals:
        if not isinstance(row, dict):
            continue
        source = str(row.get("source", "")).lower()
        symbol = str(row.get("symbol", "")).upper()
        metric = str(row.get("metric", "")).lower()
        value = number(row.get("value"))
        if source and symbol and metric and value is not None:
            grouped.setdefault((source, symbol), {})[metric] = value
    return grouped


def classify_pool(metrics, min_turnover_ratio, min_fee_tvl_ratio, min_dynamic_fee_pct):
    turnover = metrics.get("pool_top_turnover_24h_ratio")
    fee_tvl = metrics.get("pool_top_fee_tvl_ratio_24h")
    dynamic_fee = metrics.get("pool_top_dynamic_fee_pct")
    if turnover is None or fee_tvl is None or dynamic_fee is None:
        return "observe_only_missing_meteora_metrics"
    if metrics.get("pool_has_next_page", 0.0) >= 0.5:
        return "observe_only_partial_meteora_page"
    if metrics.get("pool_top_is_blacklisted", 0.0) >= 0.5:
        return "blacklisted_pool_context"
    if turnover >= min_turnover_ratio and (fee_tvl >= min_fee_tvl_ratio or dynamic_fee >= min_dynamic_fee_pct):
        return "high_fee_turnover_dlmm"
    if turnover >= min_turnover_ratio:
        return "high_turnover_dlmm"
    return "ordinary_dlmm_state"


def observe_pools(base_url, symbols, min_turnover_ratio, min_fee_tvl_ratio, min_dynamic_fee_pct, timeout):
    payload = fetch(base_url, "/v1/external/signals", {
        "categories": "defi_native_state", "sources": "meteora", "symbols": symbols or None,
    }, timeout)
    pools = []
    for (source, symbol), metrics in sorted(summarize_signals(payload.get("signals", [])).items()):
        pools.append({
            "source": source, "symbol": symbol,
            "pool_count": metrics.get("pool_count"), "has_next_page": metrics.get("pool_has_next_page"),
            "pages": metrics.get("pool_pages"), "total_count": metrics.get("pool_total_count"),
            "tvl_total": metrics.get("pool_tvl_total"), "volume_24h_total": metrics.get("pool_volume_24h_total"),
            "fees_24h_total": metrics.get("pool_fees_24h_total"), "top_tvl": metrics.get("pool_top_tvl"),
            "top_tvl_share": metrics.get("pool_top_tvl_share"), "top_volume_24h": metrics.get("pool_top_volume_24h"),
            "top_fees_24h": metrics.get("pool_top_fees_24h"), "top_fee_tvl_ratio_24h": metrics.get("pool_top_fee_tvl_ratio_24h"),
            "top_dynamic_fee_pct": metrics.get("pool_top_dynamic_fee_pct"), "top_bin_step": metrics.get("pool_top_bin_step"),
            "top_current_price": metrics.get("pool_top_current_price"),
            "top_turnover_24h_ratio": metrics.get("pool_top_turnover_24h_ratio"),
            "top_is_blacklisted": metrics.get("pool_top_is_blacklisted"),
            "state": classify_pool(metrics, min_turnover_ratio, min_fee_tvl_ratio, min_dynamic_fee_pct),
        })
    return {
        "pools": pools, "pool_count": len(pools), "upstream_errors": payload.get("errors", []),
        "evidence": ["meteora_dlmm_state_available" if pools else "missing_meteora_dlmm_state"],
        "limitations": [
            "Meteora REST values are provider snapshots, not a complete swap or LP ledger",
            "turnover, fee/TVL and dynamic-fee states are risk context, not causal alpha or LP PnL",
            "pagination is surfaced and partial pages remain observe-only",
            "no active bins, gas, MEV, routing, wallet, transaction or execution model is included",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--min-turnover-ratio", type=float, default=1.0)
    parser.add_argument("--min-fee-tvl-ratio", type=float, default=0.05)
    parser.add_argument("--min-dynamic-fee-pct", type=float, default=0.10)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if min(args.min_turnover_ratio, args.min_fee_tvl_ratio, args.min_dynamic_fee_pct) < 0 or args.timeout <= 0:
        parser.error("thresholds must be non-negative and timeout positive")
    print(json.dumps({"strategy": "crypto_meteora_dlmm_monitor", "filters": vars(args),
                      **observe_pools(args.base_url, args.symbols, args.min_turnover_ratio,
                                      args.min_fee_tvl_ratio, args.min_dynamic_fee_pct, args.timeout)},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
