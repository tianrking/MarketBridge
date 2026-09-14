#!/usr/bin/env python3
"""Record a keyed social signal beside a MarketBridge crypto quote."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "strategy"))
from python_strategy_runner import fetch  # noqa: E402


def numeric(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def select_signal(payload, source, metric, symbol):
    rows = payload.get("signals", [])
    for row in rows:
        if (str(row.get("source", "")).lower() == source.lower()
                and str(row.get("metric", "")).lower() == metric.lower()
                and (not symbol or str(row.get("symbol", "")).upper() == symbol.upper())):
            value = numeric(row.get("value"))
            if value is not None:
                return {"value": value, "source_ts_ms": row.get("source_time_ms"),
                        "signal_ts_ms": row.get("ts_ms"), "raw": row.get("raw")}
    return None


def quote_price(payload, symbol, exchange):
    for row in payload.get("quotes", []):
        instrument = row.get("instrument_ref") or {}
        source = row.get("source_ref") or {}
        if (str(instrument.get("symbol", "")).upper() != symbol.upper()
                or str(source.get("source", "")).lower() != exchange.lower()):
            continue
        values = row.get("payload") or {}
        for key in ("mark", "mid", "price", "last"):
            value = numeric(values.get(key))
            if value is not None and value > 0:
                return {"price": value, "ts_ms": (row.get("freshness") or {}).get("ts_source")}
        bid, ask = numeric(values.get("bid")), numeric(values.get("ask"))
        if bid is not None and ask is not None and bid > 0 and ask > 0:
            return {"price": (bid + ask) / 2.0,
                    "ts_ms": (row.get("freshness") or {}).get("ts_source")}
    return None


def payload_errors(payload):
    errors = []
    if isinstance(payload.get("error"), str):
        errors.append(payload["error"])
    if isinstance(payload.get("errors"), list):
        errors.extend(item for item in payload["errors"] if isinstance(item, str))
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--source", default="lunarcrush")
    parser.add_argument("--metric", default="lunarcrush_social_score")
    parser.add_argument("--signal-symbol", default="BTC")
    parser.add_argument("--price-symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--product-type", default="perp", choices=("spot", "perp"))
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=3600.0)
    parser.add_argument("--min-change", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-social-response.jsonl"))
    args = parser.parse_args()
    if (args.iterations <= 0 or args.interval_secs < 0 or args.min_change < 0
            or args.timeout <= 0):
        parser.error("iterations, timeout and social change threshold must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    previous_signal = None
    client = lambda path, params: fetch(args.base_url, path, params, args.timeout)
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            signal_payload = client("/v1/external/signals", {
                "sources": args.source, "symbols": args.signal_symbol, "metrics": args.metric,
            })
            quote_payload = client("/v1/market/quotes", {
                "symbols": args.price_symbol, "exchanges": args.exchange,
                "product_type": args.product_type, "include_stale": "false",
            })
            signal = select_signal(signal_payload, args.source, args.metric, args.signal_symbol)
            price = quote_price(quote_payload, args.price_symbol, args.exchange)
            value = signal.get("value") if signal else None
            change = value - previous_signal if value is not None and previous_signal is not None else None
            if value is not None:
                previous_signal = value
            state = ("social_rise" if change is not None and change >= args.min_change
                     else "social_fall" if change is not None and change <= -args.min_change
                     else "ordinary" if change is not None else "baseline_missing")
            observation = {
                "source": args.source, "metric": args.metric, "signal_symbol": args.signal_symbol,
                "social": signal, "social_change": change, "price": price,
                "state": state, "research_only": True,
            }
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000), "iteration": iteration + 1,
                "parameters": {"source": args.source, "metric": args.metric,
                               "signal_symbol": args.signal_symbol, "price_symbol": args.price_symbol,
                               "exchange": args.exchange, "min_change": args.min_change},
                "observation": observation,
                "upstream_errors": payload_errors(signal_payload) + payload_errors(quote_payload),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_social_signal_response_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
