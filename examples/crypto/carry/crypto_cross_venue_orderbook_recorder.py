#!/usr/bin/env python3
"""Record cross-venue order-book paper-edge snapshots to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_cross_venue_orderbook_monitor import fetch, summarize_books  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchanges", default="binance,okx,bybit")
    parser.add_argument("--market", default="spot", choices=("spot", "perp"))
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--max-skew-ms", type=int, default=2_000)
    parser.add_argument("--paper-cost-bps", type=float, default=20.0)
    parser.add_argument("--min-net-edge-bps", type=float, default=0.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=5.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-cross-venue-orderbook.jsonl"))
    args = parser.parse_args()
    if (args.target_notional <= 0 or args.max_skew_ms < 0 or args.paper_cost_bps < 0
            or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0):
        parser.error("invalid target, skew, cost, iteration, interval or timeout")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": args.symbol.upper(), "exchanges": args.exchanges,
        "market": args.market, "target_notional": args.target_notional,
        "max_skew_ms": args.max_skew_ms, "paper_cost_bps": args.paper_cost_bps,
        "min_net_edge_bps": args.min_net_edge_bps,
    }
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            payload = fetch(args.base_url, "/v1/market/order-books", {
                "market": args.market, "symbols": args.symbol, "exchanges": args.exchanges,
            }, args.timeout)
            observation = summarize_books(
                payload.get("books", []), args.target_notional, args.max_skew_ms,
                args.paper_cost_bps, args.min_net_edge_bps,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
                "upstream_errors": payload.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({
        "strategy": "crypto_cross_venue_orderbook_recorder",
        "output": str(args.output), "appended_snapshots": args.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
