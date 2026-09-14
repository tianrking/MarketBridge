#!/usr/bin/env python3
"""Record short-squeeze confluence scores beside a BTC price snapshot."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "strategy"))
from python_strategy_runner import fetch, fetch_core, score_squeeze  # noqa: E402


def quote_price(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()):
            continue
        values = row.get("payload") or {}
        for key in ("mark", "mid", "price", "last"):
            value = values.get(key)
            if isinstance(value, (int, float)) and value > 0:
                return {"price": float(value), "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = values.get("bid"), values.get("ask")
        if isinstance(bid, (int, float)) and isinstance(ask, (int, float)) and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-short-squeeze-response.jsonl"))
    args = parser.parse_args()
    if args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0:
        parser.error("iterations, interval and timeout must be valid")
    args.strategy = "squeeze"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    previous = {}
    parameters = {"symbol": args.symbol, "exchange": args.exchange}
    client = lambda path, params: fetch(args.base_url, path, params, args.timeout)
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            data = fetch_core(client, args)
            score, maximum, verdict, evidence = score_squeeze(data, args, previous)
            price_payload = client("/v1/market/quotes", {
                "symbols": args.symbol, "exchanges": args.exchange,
                "product_type": "perp", "include_stale": "false",
            })
            price = quote_price(price_payload, args.symbol, args.exchange)
            observation = {
                "score": score, "max_score": maximum,
                "state": "short_squeeze_candidate" if score >= 3 else "observe_only",
                "runner_verdict": verdict, "evidence": evidence, "price": price,
                "research_only": True,
                "upstream_errors": [payload.get("error") for payload in data.values()
                                    if isinstance(payload, dict) and payload.get("error")]
                + price_payload.get("errors", []),
            }
            handle.write(json.dumps({"recorded_at_ms": int(time.time() * 1000),
                                     "iteration": iteration + 1, "parameters": parameters,
                                     "observation": observation}, ensure_ascii=False,
                                    sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_short_squeeze_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
