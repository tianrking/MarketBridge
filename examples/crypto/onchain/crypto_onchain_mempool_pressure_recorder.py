#!/usr/bin/env python3
"""Record mempool pressure beside a synchronized MarketBridge BTC quote."""

import argparse
import json
import time
from pathlib import Path
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
        return float(value)
    except (TypeError, ValueError):
        return None


def quote_price(payload, symbol, exchange, product_type):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()
                or str(instrument.get("product_type", product_type)).lower() != product_type.lower()):
            continue
        values = row.get("payload") or {}
        for key in ("mark", "mid", "price", "last"):
            value = number(values.get(key))
            if value is not None and value > 0:
                return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = number(values.get("bid")), number(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def classify(data, high_fee, low_fee, high_vsize, low_vsize):
    fees = data.get("fee_rates_sat_vb") or {}
    fastest = number(fees.get("fastest"))
    vsize = number(data.get("mempool_vsize_mb"))
    if fastest is None or vsize is None:
        return "observe_only_missing_mempool_metrics"
    if fastest >= high_fee or vsize >= high_vsize:
        return "high_fee_pressure"
    if fastest <= low_fee and vsize <= low_vsize:
        return "low_fee_pressure"
    return "ordinary_fee_pressure"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--high-fee-sat-vb", type=float, default=20.0)
    parser.add_argument("--low-fee-sat-vb", type=float, default=3.0)
    parser.add_argument("--high-vsize-mb", type=float, default=150.0)
    parser.add_argument("--low-vsize-mb", type=float, default=25.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=60.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-onchain-mempool-pressure.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.high_fee_sat_vb < args.low_fee_sat_vb or args.low_fee_sat_vb < 0
            or args.high_vsize_mb < args.low_vsize_mb or args.low_vsize_mb < 0):
        parser.error("invalid thresholds, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "price_exchange": args.price_exchange.lower(), "price_symbol": args.price_symbol.upper(),
        "product_type": args.product_type, "high_fee_sat_vb": args.high_fee_sat_vb,
        "low_fee_sat_vb": args.low_fee_sat_vb, "high_vsize_mb": args.high_vsize_mb,
        "low_vsize_mb": args.low_vsize_mb,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            mempool_payload = fetch(args.base_url, "/v1/onchain/mempool", {}, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            data = mempool_payload.get("data") or {}
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "mempool": data,
                    "state": classify(data, args.high_fee_sat_vb, args.low_fee_sat_vb,
                                       args.high_vsize_mb, args.low_vsize_mb),
                    "price": quote_price(quote_payload, args.price_symbol,
                                          args.price_exchange, args.product_type),
                    "research_only": True,
                },
                "upstream_errors": ([mempool_payload["error"]]
                                     if mempool_payload.get("error") else [])
                + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_onchain_mempool_pressure_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
