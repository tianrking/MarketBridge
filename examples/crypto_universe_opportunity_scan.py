#!/usr/bin/env python3
"""Rank crypto perpetual research candidates from normalized universe inputs.

This scanner joins stored-kline volume/realized-volatility rows with current
funding metadata. It is a candidate discovery surface for a bounded symbol
universe, not a portfolio allocator: missing rows, unknown funding intervals
and weak evidence remain visible and no orders are placed.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(row, key):
    value = row.get(key) if isinstance(row, dict) else None
    return float(value) if isinstance(value, (int, float)) else None


def funding_map(payload, symbol_filter=None):
    result = {}
    for row in payload.get("funding", []):
        symbol = str(row.get("symbol", "")).upper()
        exchange = str(row.get("exchange", "")).lower()
        if symbol_filter and symbol not in symbol_filter:
            continue
        rate = number(row, "funding_rate")
        interval = number(row, "funding_interval_ms")
        if not symbol or not exchange or rate is None:
            continue
        hourly = rate / (interval / 3_600_000.0) if interval and interval > 0 else None
        result[(exchange, symbol)] = {
            "funding_rate_pct": rate * 100.0,
            "funding_interval_ms": int(interval) if interval and interval > 0 else None,
            "funding_hourly_pct": hourly * 100.0 if hourly is not None else None,
            "next_funding_time_ms": row.get("next_funding_time_ms"),
        }
    return result


def rank_candidates(volume_payload, volatility_payload, funding_payload, exchange,
                    min_quote_volume, min_realized_vol, min_abs_funding_hourly_pct,
                    min_score, symbol_filter=None):
    volatility = {
        (str(row.get("exchange", "")).lower(), str(row.get("symbol", "")).upper()): row
        for row in volatility_payload.get("rows", [])
    }
    funding = funding_map(funding_payload, symbol_filter)
    candidates = []
    for volume in volume_payload.get("rows", []):
        row_exchange = str(volume.get("exchange", "")).lower()
        symbol = str(volume.get("symbol", "")).upper()
        if exchange and row_exchange != exchange.lower():
            continue
        if symbol_filter and symbol not in symbol_filter:
            continue
        key = (row_exchange, symbol)
        vol_row = volatility.get(key)
        fund_row = funding.get(key)
        quote_volume = number(volume, "quote_volume")
        realized_vol = number(vol_row, "realized_volatility") if vol_row else None
        hourly_pct = fund_row.get("funding_hourly_pct") if fund_row else None
        score = 0
        evidence = []
        if quote_volume is not None and quote_volume > 0 and quote_volume >= min_quote_volume:
            score += 1
            evidence.append("quote_volume_above_threshold")
        if realized_vol is not None and realized_vol > 0 and realized_vol >= min_realized_vol:
            score += 1
            evidence.append("realized_volatility_above_threshold")
        if hourly_pct is not None and abs(hourly_pct) > 0 and abs(hourly_pct) >= min_abs_funding_hourly_pct:
            score += 1
            evidence.append("funding_magnitude_above_threshold")
        if vol_row is None:
            evidence.append("missing_matching_volatility_row")
        if fund_row is None:
            evidence.append("missing_matching_funding_row")
        elif fund_row.get("funding_interval_ms") is None:
            evidence.append("funding_interval_unknown")
        if score < min_score:
            continue
        candidates.append({
            "exchange": row_exchange,
            "symbol": symbol,
            "market": volume.get("market"),
            "quote_volume": quote_volume,
            "realized_volatility": realized_vol,
            "funding": fund_row,
            "score": score,
            "max_score": 3,
            "evidence": evidence,
        })
    candidates.sort(key=lambda row: (-row["score"], -(row["quote_volume"] or 0.0), row["symbol"]))
    return candidates


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--symbols", default="")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--min-quote-volume", type=float, default=1_000_000.0)
    parser.add_argument("--min-realized-vol", type=float, default=0.0)
    parser.add_argument("--min-abs-funding-hourly-pct", type=float, default=0.01)
    parser.add_argument("--min-score", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.limit <= 0 or options.min_quote_volume < 0 or options.min_realized_vol < 0
            or options.min_abs_funding_hourly_pct < 0 or not 0 <= options.min_score <= 3):
        parser.error("invalid limits, thresholds or score")
    symbols = {item.strip().upper() for item in options.symbols.split(",") if item.strip()}
    symbols_param = ",".join(sorted(symbols)) or None
    common = {
        "exchange": options.exchange,
        "market": options.market,
        "symbols": symbols_param,
        "interval": options.interval,
        "limit": options.limit,
    }
    volume_payload = fetch(options.base_url, "/v1/universe/top-volume", common, options.timeout)
    volatility_payload = fetch(options.base_url, "/v1/universe/volatility", common, options.timeout)
    funding_payload = fetch(options.base_url, "/v1/market/perpetual-funding", {
        "symbols": symbols_param,
        "exchanges": options.exchange,
        "active_only": "true",
        "limit": 500,
    }, options.timeout)
    candidates = rank_candidates(
        volume_payload, volatility_payload, funding_payload, options.exchange,
        options.min_quote_volume, options.min_realized_vol,
        options.min_abs_funding_hourly_pct, options.min_score, symbols or None,
    )
    evidence = [
        "volume_universe_available" if volume_payload.get("rows") else "missing_volume_universe",
        "volatility_universe_available" if volatility_payload.get("rows") else "missing_volatility_universe",
        "funding_rows_available" if funding_payload.get("funding") else "missing_funding_rows",
    ]
    print(json.dumps({
        "strategy": "crypto_universe_opportunity_scan",
        "market": {"exchange": options.exchange, "market": options.market, "interval": options.interval},
        "parameters": {"symbols": sorted(symbols), "min_quote_volume": options.min_quote_volume,
                       "min_realized_vol": options.min_realized_vol,
                       "min_abs_funding_hourly_pct": options.min_abs_funding_hourly_pct,
                       "min_score": options.min_score},
        "candidate_count": len(candidates),
        "candidates": candidates,
        "evidence": evidence,
        "upstream_errors": funding_payload.get("errors", []),
        "limitations": [
            "universe rows depend on the configured kline store and symbol universe",
            "funding is a current snapshot and is not a forward return forecast",
            "ranking excludes fees, borrow, basis, liquidity impact, capacity and hedge execution",
            "missing rows or unknown funding intervals are evidence gaps, not zero values",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
