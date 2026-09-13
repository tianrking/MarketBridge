#!/usr/bin/env python3
"""Append read-only liquidity-stress snapshots to a JSONL research archive."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_liquidity_stress_monitor import observe  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--top-levels", type=int, default=10)
    parser.add_argument("--volatility-bars", type=int, default=60)
    parser.add_argument("--ewma-alpha", type=float, default=0.2)
    parser.add_argument("--min-impact-bps", type=float, default=5.0)
    parser.add_argument("--max-spread-bps", type=float, default=2.0)
    parser.add_argument("--min-volatility-bps", type=float, default=25.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-liquidity-stress.jsonl"))
    options = parser.parse_args()
    if (options.target_notional <= 0 or options.top_levels <= 0
            or options.volatility_bars <= 1 or not 0 < options.ewma_alpha <= 1
            or min(options.min_impact_bps, options.max_spread_bps,
                   options.min_volatility_bps) < 0
            or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid target, window, threshold, iteration or interval argument")

    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "symbol": options.symbol,
        "exchange": options.exchange,
        "target_notional": options.target_notional,
        "top_levels": options.top_levels,
        "volatility_bars": options.volatility_bars,
        "ewma_alpha": options.ewma_alpha,
        "min_impact_bps": options.min_impact_bps,
        "max_spread_bps": options.max_spread_bps,
        "min_volatility_bps": options.min_volatility_bps,
    }
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe(
                options.base_url, options.symbol, options.exchange,
                options.target_notional, options.top_levels, options.volatility_bars,
                options.ewma_alpha, options.min_impact_bps, options.max_spread_bps,
                options.min_volatility_bps, options.timeout,
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
        "strategy": "crypto_liquidity_stress_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
