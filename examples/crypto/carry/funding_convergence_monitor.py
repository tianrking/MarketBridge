#!/usr/bin/env python3
"""Monitor cross-venue perpetual funding differentials through MarketBridge.

The hypothesis is deliberately narrow: if the same underlying has a persistent
funding-rate differential across venues, it is worth investigating whether the
differential survives fees, borrow, margin, transfer and execution frictions.
This script reports a gross, read-only observation and never opens a hedge.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/market/perpetual-funding?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(row, key):
    value = row.get(key)
    return float(value) if isinstance(value, (int, float)) else None


def normalize_rows(payload, symbol):
    rows = []
    for row in payload.get("funding", []):
        if str(row.get("symbol", "")).upper() != symbol.upper():
            continue
        rate = number(row, "funding_rate")
        interval = number(row, "funding_interval_ms")
        if rate is None:
            continue
        item = {
            "exchange": row.get("exchange"),
            "symbol": row.get("symbol"),
            "funding_rate": rate,
            "funding_rate_pct": rate * 100.0,
            "funding_interval_ms": int(interval) if interval and interval > 0 else None,
            "next_funding_time_ms": row.get("next_funding_time_ms"),
            "mark_price": row.get("mark_price"),
            "index_price": row.get("index_price"),
            "ts_ms": row.get("ts_ms"),
            "source": row.get("source"),
        }
        if item["funding_interval_ms"]:
            item["hourly_rate"] = rate / (item["funding_interval_ms"] / 3_600_000.0)
        else:
            item["hourly_rate"] = None
        rows.append(item)
    return rows


def observe(payload, symbol, min_spread_bps):
    rows = normalize_rows(payload, symbol)
    comparable = [row for row in rows if row["hourly_rate"] is not None]
    result = {
        "symbol": symbol,
        "rows": rows,
        "candidate": None,
        "evidence": [],
        "limitations": [
            "gross funding differential is not net PnL",
            "fees, borrow, margin, transfer latency, mark/index divergence, slippage and liquidation are not modeled",
            "a missing funding interval withholds hourly and annualized comparisons rather than inferring a schedule",
        ],
    }
    if len(comparable) < 2:
        result["evidence"].append("fewer_than_two_rows_with_known_intervals")
        return result
    low = min(comparable, key=lambda row: row["hourly_rate"])
    high = max(comparable, key=lambda row: row["hourly_rate"])
    spread_bps_per_hour = (high["hourly_rate"] - low["hourly_rate"]) * 10_000.0
    result["spread"] = {
        "low_exchange": low["exchange"],
        "high_exchange": high["exchange"],
        "spread_bps_per_hour": spread_bps_per_hour,
        "gross_annualized_bps_proxy": spread_bps_per_hour * 24.0 * 365.0,
        "annualization_basis": "hourly rates from explicit provider intervals",
    }
    if spread_bps_per_hour >= min_spread_bps:
        result["candidate"] = {
            "short_funding_exchange": high["exchange"],
            "long_funding_exchange": low["exchange"],
            "status": "investigate_only",
        }
        result["evidence"].append("cross_venue_spread_above_threshold")
    else:
        result["evidence"].append("spread_below_threshold")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchanges", default="binance,okx,bybit")
    parser.add_argument("--min-spread-bps-per-hour", type=float, default=0.5)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.iterations <= 0 or options.interval_secs < 0:
        raise SystemExit("iterations must be positive and interval-secs cannot be negative")
    if options.min_spread_bps_per_hour < 0:
        raise SystemExit("min-spread-bps-per-hour cannot be negative")

    for iteration in range(options.iterations):
        payload = fetch(options.base_url, {
            "symbols": options.symbol,
            "exchanges": options.exchanges,
            "active_only": "true",
            "limit": 100,
        }, options.timeout)
        result = observe(payload, options.symbol, options.min_spread_bps_per_hour)
        print(json.dumps({
            "strategy": "funding_convergence",
            "iteration": iteration + 1,
            "symbol": options.symbol,
            "exchanges_requested": options.exchanges.split(","),
            "supported_exchanges": payload.get("supported_exchanges", []),
            "upstream_errors": payload.get("errors", []),
            **result,
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
