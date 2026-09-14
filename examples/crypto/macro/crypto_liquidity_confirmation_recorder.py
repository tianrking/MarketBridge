#!/usr/bin/env python3
"""Freeze the liquidity-confirmation matrix beside a BTC quote."""

import argparse
import json
import time
from pathlib import Path

from crypto_liquidity_confirmation_monitor import fetch, observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--asset", default="BTC")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--coinbase-symbol", default="BTC-USD")
    parser.add_argument("--etf-threshold-musd", type=float, default=100.0)
    parser.add_argument("--stablecoin-threshold-pct", type=float, default=1.0)
    parser.add_argument("--premium-threshold-bps", type=float, default=5.0)
    parser.add_argument("--funding-threshold-rate", type=float, default=0.0005,
                        help="Funding-rate decimal threshold (0.0005 = 5 bps per interval)")
    parser.add_argument("--min-confirmations", type=int, default=2)
    parser.add_argument("--iterations", type=int, default=30)
    parser.add_argument("--interval-secs", type=float, default=900.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path,
                        default=Path("work/crypto-liquidity-confirmation.jsonl"))
    args = parser.parse_args()
    if (args.etf_threshold_musd < 0 or args.stablecoin_threshold_pct < 0
            or args.premium_threshold_bps < 0 or args.funding_threshold_rate < 0
            or not 1 <= args.min_confirmations <= 3 or args.iterations <= 0
            or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("thresholds, confirmations, iterations, interval and timeout must be valid")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "asset": args.asset, "symbol": args.symbol, "exchange": args.exchange,
        "coinbase_symbol": args.coinbase_symbol,
        "etf_threshold_musd": args.etf_threshold_musd,
        "stablecoin_threshold_pct": args.stablecoin_threshold_pct,
        "premium_threshold_bps": args.premium_threshold_bps,
        "funding_threshold_rate": args.funding_threshold_rate,
        "min_confirmations": args.min_confirmations,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
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
                "exchange": args.exchange, "symbols": args.symbol,
                "active_only": "true", "limit": 100,
            }, args.timeout)
            observation = observe(
                signal_payload, stablecoin_payload, quote_payload, funding_payload,
                asset=args.asset, etf_threshold_musd=args.etf_threshold_musd,
                stablecoin_threshold_pct=args.stablecoin_threshold_pct,
                premium_threshold_bps=args.premium_threshold_bps,
                funding_threshold_rate=args.funding_threshold_rate,
                funding_symbol=args.symbol, coinbase_symbol=args.coinbase_symbol,
                reference_symbol=args.symbol, reference_exchange=args.exchange,
                min_confirmations=args.min_confirmations,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
                "upstream_errors": signal_payload.get("errors", [])
                + stablecoin_payload.get("errors", [])
                + quote_payload.get("errors", [])
                + funding_payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_liquidity_confirmation_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
