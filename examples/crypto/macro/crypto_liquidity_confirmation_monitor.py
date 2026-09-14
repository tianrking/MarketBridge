#!/usr/bin/env python3
"""Observe a transparent multi-channel crypto liquidity context.

The case joins four public observations that are often discussed together:
daily BTC ETF flow, stablecoin supply change, Coinbase-vs-reference spot
spread, and perpetual funding crowding.  It is deliberately a context matrix,
not a price forecast.  Missing channels remain explicit and no component is
treated as an executable signal.
"""

import argparse
import json
import math
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        value = float(value)
    except (TypeError, ValueError):
        return None
    return value if math.isfinite(value) else None


def classify_etf_flow(signal_payload, asset, threshold_musd):
    for row in signal_payload.get("signals", []):
        if (str(row.get("source", "")).lower() == "farside_etf"
                and str(row.get("symbol", "")).upper() == asset.upper()):
            flow = number(row.get("value"))
            if flow is None:
                break
            if flow >= threshold_musd:
                state = "etf_inflow"
            elif flow <= -threshold_musd:
                state = "etf_outflow"
            else:
                state = "etf_ordinary"
            raw = row.get("raw") or {}
            return {"state": state, "flow_musd": flow, "date": raw.get("date"),
                    "source_time_ms": row.get("source_time_ms")}
    return {"state": "missing_etf_flow", "flow_musd": None, "date": None,
            "source_time_ms": None}


def classify_stablecoin(data, threshold_pct):
    assets = data.get("assets", []) if isinstance(data, dict) else []
    changes = [number(row.get("change_7d_pct")) for row in assets]
    changes = [value for value in changes if value is not None]
    mean_change = sum(changes) / len(changes) if changes else None
    if mean_change is None:
        state = "missing_stablecoin_supply"
    elif mean_change >= threshold_pct:
        state = "stablecoin_expansion"
    elif mean_change <= -threshold_pct:
        state = "stablecoin_contraction"
    else:
        state = "stablecoin_flat"
    return {"state": state, "mean_change_7d_pct": mean_change,
            "asset_count": len(assets),
            "total_supply_usd": number(data.get("total_supply_usd"))
            if isinstance(data, dict) else None}


def quote_symbol(row):
    instrument = row.get("instrument_ref") or {}
    return str(instrument.get("symbol") or row.get("symbol") or "").upper()


def quote_price(row):
    values = row.get("payload") or {}
    for key in ("mid", "mark", "price", "last"):
        value = number(values.get(key))
        if value is not None and value > 0:
            return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    bid, ask = number(values.get("bid")), number(values.get("ask"))
    if bid is not None and ask is not None and bid > 0 and ask > 0:
        return {"price": (bid + ask) / 2.0,
                "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def find_quote(payload, symbol, exchange, product_type="spot"):
    for row in payload.get("quotes", []):
        source = row.get("source_ref") or {}
        instrument = row.get("instrument_ref") or {}
        if (str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", product_type)).lower() != product_type.lower()
                or quote_symbol(row) != symbol.upper()):
            continue
        quote = quote_price(row)
        if quote is not None:
            return quote
    return None


def classify_coinbase_premium(quote_payload, coinbase_symbol, reference_symbol,
                              reference_exchange, threshold_bps):
    coinbase = find_quote(quote_payload, coinbase_symbol, "coinbase")
    reference = find_quote(quote_payload, reference_symbol, reference_exchange)
    premium_bps = None
    if coinbase and reference and coinbase["price"] > 0 and reference["price"] > 0:
        premium_bps = math.log(coinbase["price"] / reference["price"]) * 10_000.0
    if premium_bps is None:
        state = "missing_coinbase_spread"
    elif premium_bps >= threshold_bps:
        state = "coinbase_premium"
    elif premium_bps <= -threshold_bps:
        state = "coinbase_discount"
    else:
        state = "coinbase_ordinary"
    return {"state": state, "premium_bps": premium_bps,
            "coinbase_quote": coinbase, "reference_quote": reference}


def classify_funding(funding_payload, symbol, threshold_pct):
    rows = [row for row in funding_payload.get("funding", [])
            if not symbol or str(row.get("symbol", "")).upper() == symbol.upper()]
    if not rows:
        return {"state": "missing_funding", "funding_rate": None,
                "funding_interval_ms": None}
    row = rows[0]
    rate = number(row.get("funding_rate"))
    if rate is None:
        state = "missing_funding"
    elif rate >= threshold_pct:
        state = "crowded_long_funding"
    elif rate <= -threshold_pct:
        state = "crowded_short_funding"
    else:
        state = "funding_neutral"
    return {"state": state, "funding_rate": rate,
            "funding_interval_ms": row.get("funding_interval_ms"),
            "exchange": row.get("exchange"), "symbol": row.get("symbol")}


def classify_confirmation(etf, stablecoin, premium, min_confirmations=2):
    positive_states = {"etf_inflow", "stablecoin_expansion", "coinbase_premium"}
    negative_states = {"etf_outflow", "stablecoin_contraction", "coinbase_discount"}
    components = [etf.get("state"), stablecoin.get("state"), premium.get("state")]
    positive = [state for state in components if state in positive_states]
    negative = [state for state in components if state in negative_states]
    observed = [state for state in components if state not in {
        "missing_etf_flow", "missing_stablecoin_supply", "missing_coinbase_spread"
    }]
    if len(observed) < min_confirmations:
        state = "observe_only_insufficient_liquidity_channels"
    elif len(positive) >= min_confirmations and not negative:
        state = "risk_on_confirmation"
    elif len(negative) >= min_confirmations and not positive:
        state = "liquidity_deterioration"
    else:
        state = "mixed_liquidity_context"
    return {"state": state, "positive_components": positive,
            "negative_components": negative, "observed_components": observed,
            "score": len(positive) - len(negative),
            "min_confirmations": min_confirmations}


def observe(signal_payload, stablecoin_payload, quote_payload, funding_payload,
            asset="BTC", etf_threshold_musd=100.0, stablecoin_threshold_pct=1.0,
            premium_threshold_bps=5.0, funding_threshold_rate=0.0005,
            funding_symbol="BTCUSDT", coinbase_symbol="BTC-USD",
            reference_symbol="BTCUSDT", reference_exchange="binance",
            min_confirmations=2):
    etf = classify_etf_flow(signal_payload, asset, etf_threshold_musd)
    stablecoin = classify_stablecoin(stablecoin_payload.get("data") or {}, stablecoin_threshold_pct)
    premium = classify_coinbase_premium(
        quote_payload, coinbase_symbol, reference_symbol, reference_exchange,
        premium_threshold_bps,
    )
    funding = classify_funding(funding_payload, funding_symbol, funding_threshold_rate)
    confirmation = classify_confirmation(etf, stablecoin, premium, min_confirmations)
    price = find_quote(quote_payload, reference_symbol, reference_exchange)
    return {
        "state": confirmation["state"], "confirmation": confirmation,
        "etf": etf, "stablecoin": stablecoin, "coinbase": premium,
        "funding": funding, "price": price,
        "evidence": [
            "etf_flow_channel_available" if etf["state"] != "missing_etf_flow" else "missing_etf_flow_channel",
            "stablecoin_supply_channel_available" if stablecoin["state"] != "missing_stablecoin_supply" else "missing_stablecoin_supply_channel",
            "coinbase_spread_channel_available" if premium["state"] != "missing_coinbase_spread" else "missing_coinbase_spread_channel",
            "funding_channel_available" if funding["state"] != "missing_funding" else "missing_funding_channel",
        ],
        "limitations": [
            "the four channels have different publication clocks and are not a synchronized causal factor",
            "Coinbase USD versus reference USDT can contain a quote-unit basis",
            "stablecoin circulating supply is not exchange inventory or deployable liquidity",
            "ETF flow and funding observations do not establish ownership, intent or executable demand",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--asset", default="BTC")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--coinbase-symbol", default="BTC-USD")
    parser.add_argument("--premium-threshold-bps", type=float, default=5.0)
    parser.add_argument("--etf-threshold-musd", type=float, default=100.0)
    parser.add_argument("--stablecoin-threshold-pct", type=float, default=1.0)
    parser.add_argument("--funding-threshold-rate", type=float, default=0.0005,
                        help="Funding-rate decimal threshold (0.0005 = 5 bps per interval)")
    parser.add_argument("--min-confirmations", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.etf_threshold_musd < 0 or args.stablecoin_threshold_pct < 0
            or args.premium_threshold_bps < 0 or args.funding_threshold_rate < 0
            or not 1 <= args.min_confirmations <= 3 or args.timeout <= 0):
        parser.error("thresholds, min-confirmations and timeout must be valid")
    signal_payload = fetch(args.base_url, "/v1/external/signals", {
        "sources": "farside_etf", "symbols": args.asset,
    }, args.timeout)
    stablecoin_payload = fetch(args.base_url, "/v1/external/stablecoins", {
        "peg_type": "peggedUSD", "limit": 100,
    }, args.timeout)
    quote_payload = fetch(args.base_url, "/v1/market/quotes", {
        "exchanges": f"coinbase,{args.exchange}", "product_type": "spot",
        "include_stale": "false",
    }, args.timeout)
    funding_payload = fetch(args.base_url, "/v1/market/perpetual-funding", {
        "exchange": args.exchange, "symbols": args.symbol, "active_only": "true", "limit": 100,
    }, args.timeout)
    result = observe(
        signal_payload, stablecoin_payload, quote_payload, funding_payload,
        asset=args.asset, etf_threshold_musd=args.etf_threshold_musd,
        stablecoin_threshold_pct=args.stablecoin_threshold_pct,
        premium_threshold_bps=args.premium_threshold_bps,
        funding_threshold_rate=args.funding_threshold_rate,
        funding_symbol=args.symbol, coinbase_symbol=args.coinbase_symbol,
        reference_symbol=args.symbol, reference_exchange=args.exchange,
        min_confirmations=args.min_confirmations,
    )
    result.update({
        "strategy": "crypto_liquidity_confirmation_monitor",
        "observed_at_ms": int(time.time() * 1000),
        "parameters": vars(args),
        "upstream_errors": signal_payload.get("errors", [])
        + stablecoin_payload.get("errors", [])
        + quote_payload.get("errors", [])
        + funding_payload.get("errors", []),
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
