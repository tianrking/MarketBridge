#!/usr/bin/env python3
"""Observe read-only Raydium pool fragmentation and turnover states.

The falsifiable hypothesis is narrow: a Raydium token pair with high 24-hour
volume relative to page TVL and a low top-pool TVL share may represent a more
fragmented execution-risk regime than an ordinary or concentrated pair. The
monitor consumes only MarketBridge ``defi_native_state`` metrics; it does not
estimate LP returns, route fills, or execute a swap.
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


def classify_pool(metrics, min_turnover_ratio, max_top_share, min_coverage_ratio):
    turnover = metrics.get("pool_turnover_24h_page_ratio")
    top_share = metrics.get("pool_top_tvl_share_page")
    coverage = metrics.get("pool_page_coverage_ratio")
    has_next = metrics.get("pool_has_next_page")
    if turnover is None or top_share is None:
        return "observe_only_missing_pool_metrics"
    if coverage is None or has_next is None or has_next >= 0.5 or coverage < min_coverage_ratio:
        return "observe_only_partial_pool_catalog"
    if turnover >= min_turnover_ratio and top_share <= max_top_share:
        return "high_turnover_fragmented"
    if turnover >= min_turnover_ratio:
        return "high_turnover_concentrated"
    return "ordinary_pool_state"


def observe_pools(base_url, symbols, min_turnover_ratio, max_top_share,
                  min_coverage_ratio, timeout):
    payload = fetch(base_url, "/v1/external/signals", {
        "categories": "defi_native_state",
        "sources": "raydium",
        "symbols": symbols or None,
    }, timeout)
    pools = []
    for (source, symbol), metrics in sorted(summarize_signals(payload.get("signals", [])).items()):
        turnover = metrics.get("pool_turnover_24h_page_ratio")
        top_share = metrics.get("pool_top_tvl_share_page")
        pools.append({
            "source": source,
            "symbol": symbol,
            "page_count": metrics.get("pool_page_count"),
            "catalog_count": metrics.get("pool_catalog_count"),
            "has_next_page": metrics.get("pool_has_next_page"),
            "page_coverage_ratio": metrics.get("pool_page_coverage_ratio"),
            "page_tvl_usd": metrics.get("pool_tvl_usd_page_total"),
            "page_volume_24h_usd": metrics.get("pool_volume_24h_usd_page_total"),
            "page_fee_24h_usd": metrics.get("pool_fee_24h_usd_page_total"),
            "top_tvl_usd": metrics.get("pool_top_tvl_usd"),
            "top_tvl_share_page": top_share,
            "top_volume_24h_usd": metrics.get("pool_top_volume_24h_usd"),
            "top_volume_share_page": metrics.get("pool_top_volume_share_page"),
            "top_fee_rate": metrics.get("pool_top_fee_rate"),
            "top_price": metrics.get("pool_top_price"),
            "turnover_24h_page_ratio": turnover,
            "top_turnover_24h_ratio": metrics.get("pool_top_turnover_24h_ratio"),
            "state": classify_pool(metrics, min_turnover_ratio, max_top_share, min_coverage_ratio),
        })
    return {
        "pools": pools,
        "pool_count": len(pools),
        "upstream_errors": payload.get("errors", []),
        "evidence": ["raydium_pool_state_available" if pools else "missing_raydium_pool_state"],
        "limitations": [
            "Raydium API values are provider snapshots and page aggregates, not a complete swap ledger",
            "top-pool shares are page-level and remain observe-only when pagination is incomplete",
            "turnover is not LP APR, impermanent loss, realized slippage or executable route depth",
            "no gas, MEV, wallet, transaction, signing or order path is included",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbols", default=None, help="comma-separated Raydium symbols")
    parser.add_argument("--min-turnover-ratio", type=float, default=1.0)
    parser.add_argument("--max-top-share", type=float, default=0.65)
    parser.add_argument("--min-coverage-ratio", type=float, default=1.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.min_turnover_ratio < 0 or not 0 <= args.max_top_share <= 1
            or not 0 <= args.min_coverage_ratio <= 1 or args.timeout <= 0):
        parser.error("invalid turnover, share, coverage or timeout threshold")
    print(json.dumps({
        "strategy": "crypto_raydium_pool_concentration_monitor",
        "filters": {
            "symbols": args.symbols,
            "min_turnover_ratio": args.min_turnover_ratio,
            "max_top_share": args.max_top_share,
            "min_coverage_ratio": args.min_coverage_ratio,
        },
        **observe_pools(args.base_url, args.symbols, args.min_turnover_ratio,
                        args.max_top_share, args.min_coverage_ratio, args.timeout),
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
