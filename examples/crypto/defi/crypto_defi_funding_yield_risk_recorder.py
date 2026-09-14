#!/usr/bin/env python3
"""Record DeFi reward-yield and perpetual funding risk context to JSONL."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_defi_funding_yield_risk_monitor import summarize


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    suffix = f"?{query}" if query else ""
    request = Request(f"{base_url.rstrip('/')}{path}{suffix}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--projects", default="ethena-usde,aave-v3,morpho-blue,pendle-v2,curve-dex,convex-finance")
    parser.add_argument("--symbols", default="USDE,SUSDE")
    parser.add_argument("--funding-symbols", default="BTCUSDT,ETHUSDT,SOLUSDT")
    parser.add_argument("--funding-exchange", default=None)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--reward-share-threshold", type=float, default=0.5)
    parser.add_argument("--negative-funding-threshold-pct", type=float, default=0.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=600.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-defi-funding-yield-risk.jsonl"))
    args = parser.parse_args()
    if args.limit <= 0 or args.limit > 500 or args.iterations <= 0 or args.interval_secs < 0 or args.timeout <= 0:
        parser.error("limit, iterations, interval and timeout must be valid")
    funding_symbols = [item.strip().upper() for item in args.funding_symbols.split(",") if item.strip()]
    if not funding_symbols:
        parser.error("funding-symbols must not be empty")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"projects": args.projects, "symbols": args.symbols,
                  "funding_symbols": funding_symbols, "funding_exchange": args.funding_exchange,
                  "limit": args.limit, "reward_share_threshold": args.reward_share_threshold,
                  "negative_funding_threshold_pct": args.negative_funding_threshold_pct}
    with args.output.open("a", encoding="utf-8") as handle:
        for iteration in range(args.iterations):
            yields = fetch(args.base_url, "/v1/external/defi-yields", {
                "projects": args.projects, "symbols": args.symbols, "limit": args.limit,
            }, args.timeout)
            funding = fetch(args.base_url, "/v1/market/perpetual-funding", {
                "exchange": args.funding_exchange, "symbols": ",".join(funding_symbols),
                "active_only": "true", "limit": 100,
            }, args.timeout)
            observation = summarize(yields.get("data") or {}, funding, funding_symbols,
                                    args.reward_share_threshold, args.negative_funding_threshold_pct)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
                "upstream_errors": yields.get("errors", []) + funding.get("errors", []),
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < args.iterations:
                time.sleep(args.interval_secs)
    print(json.dumps({"strategy": "crypto_defi_funding_yield_risk_recorder",
                      "output": str(args.output), "appended_snapshots": args.iterations,
                      "execution": "research_only_no_orders"}, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
