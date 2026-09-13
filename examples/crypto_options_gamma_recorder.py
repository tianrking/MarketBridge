#!/usr/bin/env python3
"""Record read-only crypto option gamma-map snapshots as JSONL."""

import argparse
import json
import time
from pathlib import Path

from crypto_options_gamma_monitor import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--venue", default="deribit")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--min-near-share", type=float, default=0.50)
    parser.add_argument("--min-concentration", type=float, default=0.10)
    parser.add_argument("--max-book-fetches", type=int, default=24)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-options-gamma.jsonl"))
    options = parser.parse_args()
    if (options.expiry_days <= 0 or not 0 < options.atm_band < 0.25
            or not 0 <= options.min_near_share <= 1
            or not 0 <= options.min_concentration <= 1
            or options.max_book_fetches < 0
            or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid expiry, band, thresholds, iterations or interval")
    options.output.parent.mkdir(parents=True, exist_ok=True)
    parameters = {
        "currency": options.currency,
        "venue": options.venue,
        "expiry_days": options.expiry_days,
        "atm_band": options.atm_band,
        "min_near_share": options.min_near_share,
        "min_concentration": options.min_concentration,
        "max_book_fetches": options.max_book_fetches,
    }
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe(options.base_url, options.currency, options.venue,
                                  options.expiry_days, options.atm_band,
                                  options.min_near_share, options.min_concentration,
                                  options.max_book_fetches,
                                  options.timeout)
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
        "strategy": "crypto_options_gamma_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
