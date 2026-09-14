#!/usr/bin/env python3
"""Append current spot/perp basis snapshots to a JSONL research archive."""

import argparse
import json
import time
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def observe(base_url, symbol, exchanges, timeout):
    basis = fetch(base_url, "/v1/market/basis", {
        "symbols": symbol, "exchanges": ",".join(exchanges),
    }, timeout)
    funding = fetch(base_url, "/v1/market/perpetual-funding", {
        "symbols": symbol, "exchanges": ",".join(exchanges),
        "active_only": "true", "limit": 100,
    }, timeout)
    return {
        "basis": basis.get("basis", []),
        "funding": funding.get("funding", []),
        "upstream_errors": basis.get("errors", []) + funding.get("errors", []),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchanges", default="binance,okx")
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-basis.jsonl"))
    options = parser.parse_args()
    exchanges = [item.strip().lower() for item in options.exchanges.split(",") if item.strip()]
    if not exchanges or options.iterations <= 0 or options.interval_secs < 0:
        parser.error("exchanges must be non-empty, iterations positive and interval non-negative")
    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {"symbol": options.symbol, "exchanges": exchanges}
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe(options.base_url, options.symbol, exchanges, options.timeout)
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "parameters": parameters,
                "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < options.iterations:
                time.sleep(options.interval_secs)
    print(json.dumps({
        "strategy": "crypto_basis_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
