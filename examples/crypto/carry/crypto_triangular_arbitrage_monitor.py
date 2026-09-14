#!/usr/bin/env python3
"""Measure paper triangular quote inconsistencies without executing trades."""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def normalize_quotes(rows, exchange):
    quotes = {}
    for row in rows:
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        symbol = str(row.get("symbol") or instrument.get("symbol") or "").upper()
        venue = str(row.get("exchange") or source.get("source") or "").lower()
        if not symbol or venue != exchange.lower():
            continue
        payload = row.get("payload") or {}
        bid, ask = number(payload.get("bid")), number(payload.get("ask"))
        freshness = row.get("freshness") or {}
        if bid is None or ask is None or bid <= 0 or ask <= 0 or ask < bid:
            continue
        quotes[symbol] = {
            "bid": bid,
            "ask": ask,
            "ts_ms": freshness.get("ts_source"),
        }
    return quotes


def path_result(name, legs, start_notional, paper_cost_bps, max_skew_ms):
    if not all(leg in legs for leg in ("btc_usdt", "eth_btc", "eth_usdt")):
        return None
    timestamps = [legs[key]["ts_ms"] for key in ("btc_usdt", "eth_btc", "eth_usdt")]
    if not all(isinstance(value, int) for value in timestamps):
        return None
    skew_ms = max(timestamps) - min(timestamps)
    if skew_ms > max_skew_ms:
        return None
    btc_usdt, eth_btc, eth_usdt = legs["btc_usdt"], legs["eth_btc"], legs["eth_usdt"]
    if name == "USDT->BTC->ETH->USDT":
        final_notional = start_notional / btc_usdt["ask"] / eth_btc["ask"] * eth_usdt["bid"]
    else:
        final_notional = start_notional / eth_usdt["ask"] * eth_btc["bid"] * btc_usdt["bid"]
    gross_edge_bps = (final_notional / start_notional - 1.0) * 10_000.0
    net_final = final_notional * (1.0 - paper_cost_bps / 10_000.0) ** 3
    net_edge_bps = (net_final / start_notional - 1.0) * 10_000.0
    return {
        "path": name,
        "start_notional": start_notional,
        "gross_final_notional": final_notional,
        "gross_edge_bps": gross_edge_bps,
        "paper_cost_bps_per_leg": paper_cost_bps,
        "net_edge_bps": net_edge_bps,
        "timestamp_skew_ms": skew_ms,
    }


def summarize_quotes(rows, exchange, start_notional, paper_cost_bps, max_skew_ms, min_net_edge_bps):
    quotes = normalize_quotes(rows, exchange)
    legs = {
        "btc_usdt": quotes.get("BTCUSDT"),
        "eth_btc": quotes.get("ETHBTC"),
        "eth_usdt": quotes.get("ETHUSDT"),
    }
    paths = [result for result in (
        path_result("USDT->BTC->ETH->USDT", legs, start_notional, paper_cost_bps, max_skew_ms),
        path_result("USDT->ETH->BTC->USDT", legs, start_notional, paper_cost_bps, max_skew_ms),
    ) if result is not None]
    paths.sort(key=lambda row: row["net_edge_bps"], reverse=True)
    best = paths[0] if paths else None
    state = "observe_only_missing_synchronized_triangle" if best is None else (
        "triangular_quote_edge" if best["net_edge_bps"] >= min_net_edge_bps
        else "triangular_edge_below_paper_hurdle"
    )
    return {
        "state": state,
        "available_symbols": sorted(quotes),
        "paths": paths,
        "best_path": best,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--start-notional", type=float, default=10_000.0)
    parser.add_argument("--max-skew-ms", type=int, default=500)
    parser.add_argument("--paper-cost-bps-per-leg", type=float, default=10.0)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.start_notional <= 0 or args.max_skew_ms < 0
            or args.paper_cost_bps_per_leg < 0 or args.timeout <= 0):
        parser.error("notional, skew, cost or timeout arguments are invalid")
    payload = fetch(args.base_url, "/v1/market/quotes", {
        "symbols": "BTCUSDT,ETHBTC,ETHUSDT", "exchanges": args.exchange,
        "product_type": "spot", "include_stale": "false",
    }, args.timeout)
    summary = summarize_quotes(
        payload.get("quotes", []), args.exchange, args.start_notional,
        args.paper_cost_bps_per_leg, args.max_skew_ms, args.min_net_edge_bps,
    )
    print(json.dumps({
        "strategy": "crypto_triangular_arbitrage_monitor",
        "market": {"exchange": args.exchange, "product_type": "spot",
                   "symbols": ["BTCUSDT", "ETHBTC", "ETHUSDT"]},
        "filters": {"start_notional": args.start_notional,
                    "max_skew_ms": args.max_skew_ms,
                    "paper_cost_bps_per_leg": args.paper_cost_bps_per_leg,
                    "min_net_edge_bps": args.min_net_edge_bps},
        "summary": summary,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "top-of-book quotes are not depth-aware fills and do not prove cycle completion",
            "paper cost is a sensitivity input, not venue fees, slippage or latency",
            "three legs can move between observations; inventory, queue and execution are absent",
            "no order, wallet, allocation or transfer path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
