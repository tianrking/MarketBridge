#!/usr/bin/env python3
"""Observe DEX-pool turnover and liquidity pressure from MarketBridge.

The falsifiable hypothesis is deliberately operational: a pool with unusually
large recent swap volume relative to reported liquidity is a higher execution-
risk regime than a liquid, low-turnover pool.  This is a pool-state monitor,
not an LP-yield estimate, routing recommendation, or wallet action.
"""

import argparse
import json
import math

from crypto_microstructure_monitor import fetch


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def quote_identity(row):
    instrument = row.get("instrument_ref") or {}
    source = row.get("source_ref") or {}
    symbol = instrument.get("symbol")
    exchange = source.get("source")
    if not isinstance(symbol, str) or not isinstance(exchange, str):
        return None
    payload = row.get("payload") or {}
    bid = number(payload.get("bid"))
    ask = number(payload.get("ask"))
    mark = number(payload.get("mark"))
    mid = ((bid + ask) / 2.0) if bid and ask and bid > 0 and ask > 0 else mark
    return exchange.lower(), symbol.upper(), {
        "mid": mid,
        "ts_ms": row.get("freshness", {}).get("ts_source"),
        "stale": bool((row.get("freshness") or {}).get("stale", False)),
    }


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


def classify_pool(metrics, min_liquidity_usd, min_turnover_h1):
    liquidity = metrics.get("pool_liquidity_usd")
    volume = metrics.get("swap_volume_h1")
    if liquidity is None or liquidity <= 0 or volume is None or volume < 0:
        return "observe_only_missing_liquidity_or_volume"
    turnover = volume / liquidity
    if turnover >= min_turnover_h1 and liquidity < min_liquidity_usd:
        return "thin_liquidity_high_flow"
    if turnover >= min_turnover_h1:
        return "high_turnover_pool"
    return "normal_pool_activity"


def observe_pools(base_url, sources, symbols, min_liquidity_usd, min_turnover_h1, timeout):
    signal_payload = fetch(base_url, "/v1/external/signals", {
        "categories": "defi_native_state",
        "sources": sources or None,
        "symbols": symbols or None,
    }, timeout)
    quote_payload = fetch(base_url, "/v1/market/quotes", {
        "product_type": "dex_pool",
        "exchanges": sources or None,
        "symbols": symbols or None,
        "include_stale": "false",
    }, timeout)
    quotes = {}
    for row in quote_payload.get("quotes", []):
        identity = quote_identity(row)
        if identity:
            source, symbol, details = identity
            quotes[(source, symbol)] = details

    pools = []
    for (source, symbol), metrics in sorted(summarize_signals(signal_payload.get("signals", [])).items()):
        liquidity = metrics.get("pool_liquidity_usd")
        volume_h1 = metrics.get("swap_volume_h1")
        buy_count = metrics.get("swap_buys_h1")
        sell_count = metrics.get("swap_sells_h1")
        total_count = (buy_count + sell_count) if buy_count is not None and sell_count is not None else None
        turnover = volume_h1 / liquidity if liquidity and liquidity > 0 and volume_h1 is not None else None
        pools.append({
            "source": source,
            "symbol": symbol,
            "liquidity_usd": liquidity,
            "swap_volume_h1_usd": volume_h1,
            "swap_volume_h24_usd": metrics.get("swap_volume_h24"),
            "turnover_h1": turnover,
            "swap_buys_h1": buy_count,
            "swap_sells_h1": sell_count,
            "buy_sell_imbalance_h1": ((buy_count - sell_count) / total_count)
            if total_count and total_count > 0 else None,
            "quote": quotes.get((source, symbol)),
            "state": classify_pool(metrics, min_liquidity_usd, min_turnover_h1),
        })
    return {
        "pools": pools,
        "pool_count": len(pools),
        "upstream_errors": signal_payload.get("errors", []) + quote_payload.get("errors", []),
        "evidence": [
            "defi_native_state_available" if pools else "missing_defi_native_state",
            "dex_pool_quote_available" if quotes else "missing_dex_pool_quote",
        ],
        "limitations": [
            "DexScreener/native-state metrics are provider snapshots, not a complete on-chain swap ledger",
            "volume-to-liquidity turnover is not fee APR, impermanent-loss or LP PnL",
            "no route quote, price-impact simulation, gas estimate or wallet action is performed",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--sources", default=None, help="comma-separated DEX sources")
    parser.add_argument("--symbols", default=None, help="comma-separated pool symbols")
    parser.add_argument("--min-liquidity-usd", type=float, default=100_000.0)
    parser.add_argument("--min-turnover-h1", type=float, default=0.25)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.min_liquidity_usd < 0 or options.min_turnover_h1 < 0 or options.timeout <= 0:
        parser.error("liquidity, turnover and timeout thresholds must be non-negative/positive")
    result = observe_pools(
        options.base_url, options.sources, options.symbols,
        options.min_liquidity_usd, options.min_turnover_h1, options.timeout,
    )
    print(json.dumps({
        "strategy": "crypto_defi_pool_flow_monitor",
        "filters": {
            "sources": options.sources,
            "symbols": options.symbols,
            "min_liquidity_usd": options.min_liquidity_usd,
            "min_turnover_h1": options.min_turnover_h1,
        },
        **result,
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
