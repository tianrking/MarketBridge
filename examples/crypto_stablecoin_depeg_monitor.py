#!/usr/bin/env python3
"""Observe stablecoin quote deviations and liquidity-risk context.

This read-only monitor asks whether selected stablecoin pairs are still close
to a one-for-one quote.  It can include a risk-asset quote (BTCUSDT by default)
for later event-study replay, but it never treats a deviation as an arbitrage
instruction and never executes a trade.
"""

import argparse
import json
import math
from urllib.parse import urlencode
from urllib.request import Request, urlopen


STABLE_ASSETS = {
    "DAI", "FDUSD", "FRAX", "LUSD", "PYUSD", "USDC", "USDD", "USDE",
    "USDP", "USDS", "USDT", "TUSD", "USDG", "GUSD", "USD", "USDA",
}


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def split_stable_pair(symbol):
    compact = "".join(character for character in str(symbol).upper() if character.isalnum())
    for base in sorted(STABLE_ASSETS - {"USD"}, key=len, reverse=True):
        if not compact.startswith(base):
            continue
        quote = compact[len(base):]
        if quote in STABLE_ASSETS:
            return base, quote
    return None


def quote_identity(row):
    instrument = row.get("instrument_ref") or {}
    source = row.get("source_ref") or {}
    symbol = row.get("symbol") or instrument.get("symbol")
    exchange = row.get("exchange") or source.get("source")
    if not symbol or not exchange:
        return None
    payload = row.get("payload") or {}
    bid, ask = number(payload.get("bid")), number(payload.get("ask"))
    mid = number(payload.get("mid")) or number(payload.get("mark")) or number(payload.get("price"))
    if mid is None and bid is not None and ask is not None and bid > 0 and ask > 0:
        mid = (bid + ask) / 2.0
    if mid is None or mid <= 0 or not math.isfinite(mid):
        return None
    spread_bps = ((ask / bid) - 1.0) * 10_000.0 if bid and ask and ask >= bid > 0 else None
    return {
        "symbol": str(symbol).upper(),
        "exchange": str(exchange).lower(),
        "base_quote": split_stable_pair(symbol),
        "mid": mid,
        "bid": bid,
        "ask": ask,
        "spread_bps": spread_bps,
        "ts_ms": (row.get("freshness") or {}).get("ts_source"),
        "stale": bool((row.get("freshness") or {}).get("stale", False)),
    }


def stablecoin_rows(rows):
    normalized = []
    for row in rows:
        quote = quote_identity(row)
        if quote and quote["base_quote"]:
            base, counter = quote["base_quote"]
            quote["base"] = base
            quote["counter"] = counter
            quote["deviation_bps"] = (quote["mid"] - 1.0) * 10_000.0
            quote["abs_deviation_bps"] = abs(quote["deviation_bps"])
            normalized.append(quote)
    return sorted(normalized, key=lambda row: row["abs_deviation_bps"], reverse=True)


def risk_asset_row(rows, symbol, exchange):
    for row in rows:
        quote = quote_identity(row)
        if quote and quote["symbol"] == symbol.upper() and quote["exchange"] == exchange.lower():
            return {key: quote[key] for key in ("symbol", "exchange", "mid", "ts_ms", "stale")}
    return None


def summarize_quotes(rows, risk_symbol, risk_exchange, watch_bps, stress_bps, spread_bps):
    stable = stablecoin_rows(rows)
    risk = risk_asset_row(rows, risk_symbol, risk_exchange)
    worst = stable[0] if stable else None
    if worst is None:
        state = "observe_only_missing_stablecoin_quotes"
    elif worst["abs_deviation_bps"] >= stress_bps or any(
            row["spread_bps"] is not None and row["spread_bps"] >= spread_bps for row in stable):
        state = "stablecoin_depeg_stress"
    elif worst["abs_deviation_bps"] >= watch_bps:
        state = "stablecoin_depeg_watch"
    else:
        state = "stablecoin_within_band"
    return {
        "state": state,
        "stablecoin_quotes": stable,
        "worst_quote": worst,
        "risk_asset": risk,
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="spot", choices=("spot", "dex_pool"))
    parser.add_argument("--stable-symbols", default="USDTUSDC,USDCUSDT,DAIUSDT")
    parser.add_argument("--risk-symbol", default="BTCUSDT")
    parser.add_argument("--watch-bps", type=float, default=20.0)
    parser.add_argument("--stress-bps", type=float, default=50.0)
    parser.add_argument("--max-spread-bps", type=float, default=40.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = ",".join(dict.fromkeys(
        item.strip().upper() for item in f"{args.stable_symbols},{args.risk_symbol}".split(",") if item.strip()
    ))
    if (args.watch_bps < 0 or args.stress_bps < args.watch_bps or args.max_spread_bps < 0
            or args.timeout <= 0):
        parser.error("deviation thresholds or timeout are invalid")
    payload = fetch(args.base_url, "/v1/market/quotes", {
        "symbols": symbols, "exchanges": args.exchange, "product_type": args.product_type,
        "include_stale": "false",
    }, args.timeout)
    summary = summarize_quotes(
        payload.get("quotes", []), args.risk_symbol, args.exchange,
        args.watch_bps, args.stress_bps, args.max_spread_bps,
    )
    print(json.dumps({
        "strategy": "crypto_stablecoin_depeg_monitor",
        "market": {"exchange": args.exchange, "product_type": args.product_type,
                   "stable_symbols": symbols, "risk_symbol": args.risk_symbol},
        "filters": {"watch_bps": args.watch_bps, "stress_bps": args.stress_bps,
                    "max_spread_bps": args.max_spread_bps},
        "summary": summary,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "a quote deviation is not proof of insolvency, redemption pressure or arbitrageability",
            "one venue or pool is not a complete stablecoin market and quote coverage is provider-dependent",
            "spread is a top-of-book proxy; depth, conversion, fees, latency and settlement are absent",
            "no order, wallet, liquidity withdrawal or allocation path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
