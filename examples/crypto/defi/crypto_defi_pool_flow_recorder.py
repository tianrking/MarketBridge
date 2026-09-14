#!/usr/bin/env python3
"""Append read-only DEX-pool flow/liquidity observations to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from crypto_defi_pool_flow_monitor import observe_pools


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--sources", default=None)
    parser.add_argument("--symbols", default=None)
    parser.add_argument("--min-liquidity-usd", type=float, default=100_000.0)
    parser.add_argument("--min-turnover-h1", type=float, default=0.25)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-defi-pool-flow.jsonl"))
    options = parser.parse_args()
    if (options.min_liquidity_usd < 0 or options.min_turnover_h1 < 0
            or options.iterations <= 0 or options.interval_secs < 0 or options.timeout <= 0):
        parser.error("invalid liquidity, turnover, iteration, interval or timeout arguments")
    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "sources": options.sources,
        "symbols": options.symbols,
        "min_liquidity_usd": options.min_liquidity_usd,
        "min_turnover_h1": options.min_turnover_h1,
    }
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe_pools(
                options.base_url, options.sources, options.symbols,
                options.min_liquidity_usd, options.min_turnover_h1, options.timeout,
            )
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
        "strategy": "crypto_defi_pool_flow_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
