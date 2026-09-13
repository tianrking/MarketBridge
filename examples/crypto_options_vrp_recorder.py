#!/usr/bin/env python3
"""Append read-only crypto options IV-minus-RV snapshots to JSONL."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_options_vrp_monitor import observe_vrp


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--rv-bars", type=int, default=168)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--wing-min", type=float, default=0.85)
    parser.add_argument("--wing-max", type=float, default=1.15)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-term-slope-iv", type=float, default=3.0)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-vrp.jsonl"))
    options = parser.parse_args()
    if (options.expiry_days <= 0 or options.rv_bars <= 1 or options.iterations <= 0
            or options.interval_secs < 0 or options.vrp_threshold < 0
            or not 0 < options.atm_band < 0.25 or not 0 < options.wing_min < 1
            or options.wing_max <= 1 or options.wing_min >= options.wing_max
            or options.min_skew_iv < 0 or options.min_term_slope_iv < 0):
        parser.error("invalid expiry, RV window, moneyness, threshold, iteration or interval arguments")

    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "currency": options.currency,
        "venue": options.venue,
        "expiry_days": options.expiry_days,
        "price_exchange": options.price_exchange,
        "symbol": options.symbol,
        "market": options.market,
        "interval": options.interval,
        "rv_bars": options.rv_bars,
        "atm_band": options.atm_band,
        "wing_min": options.wing_min,
        "wing_max": options.wing_max,
        "min_skew_iv": options.min_skew_iv,
        "min_term_slope_iv": options.min_term_slope_iv,
        "vrp_threshold": options.vrp_threshold,
    }
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe_vrp(
                options.base_url, options.currency, options.venue, options.expiry_days,
                options.price_exchange, options.symbol, options.market, options.interval,
                options.rv_bars, options.atm_band, options.wing_min, options.wing_max,
                options.min_skew_iv, options.min_term_slope_iv, options.vrp_threshold,
                options.timeout,
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
        "strategy": "crypto_options_vrp_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
