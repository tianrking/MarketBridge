#!/usr/bin/env python3
"""Record mining-pressure context beside a synchronized MarketBridge BTC quote."""

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


def classify(data, stress_difficulty, stress_hashrate, tailwind_difficulty, tailwind_hashrate):
    difficulty = number(data.get("difficulty_change_pct"))
    hashrate = number(data.get("hashrate_change_7d_pct"))
    if difficulty is None or hashrate is None:
        return "observe_only_missing_mining_metrics"
    if difficulty <= stress_difficulty or hashrate <= stress_hashrate:
        return "miner_stress_context"
    if difficulty >= tailwind_difficulty and hashrate >= tailwind_hashrate:
        return "miner_tailwind_context"
    return "ordinary_mining_context"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--stress-difficulty-pct", type=float, default=-3.0)
    parser.add_argument("--stress-hashrate-pct", type=float, default=-3.0)
    parser.add_argument("--tailwind-difficulty-pct", type=float, default=3.0)
    parser.add_argument("--tailwind-hashrate-pct", type=float, default=3.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-onchain-mining-pressure.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0
            or args.stress_difficulty_pct > args.tailwind_difficulty_pct
            or args.stress_hashrate_pct > args.tailwind_hashrate_pct):
        parser.error("invalid thresholds, iteration, interval or timeout arguments")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "price_exchange": args.price_exchange.lower(), "price_symbol": args.price_symbol.upper(),
        "product_type": args.product_type, "stress_difficulty_pct": args.stress_difficulty_pct,
        "stress_hashrate_pct": args.stress_hashrate_pct,
        "tailwind_difficulty_pct": args.tailwind_difficulty_pct,
        "tailwind_hashrate_pct": args.tailwind_hashrate_pct,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            mining_payload = fetch(args.base_url, "/v1/onchain/mining", {}, args.timeout)
            quote_payload = fetch(args.base_url, "/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.price_exchange,
                "product_type": args.product_type, "include_stale": "false",
            }, args.timeout)
            data = mining_payload.get("data") or {}
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": parameters,
                "observation": {
                    "mining": data,
                    "state": classify(data, args.stress_difficulty_pct, args.stress_hashrate_pct,
                                       args.tailwind_difficulty_pct, args.tailwind_hashrate_pct),
                    "price": quote_price(quote_payload, args.price_symbol,
                                          args.price_exchange, args.product_type),
                    "research_only": True,
                },
                "upstream_errors": ([mining_payload["error"]]
                                     if mining_payload.get("error") else [])
                + quote_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_onchain_mining_pressure_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
