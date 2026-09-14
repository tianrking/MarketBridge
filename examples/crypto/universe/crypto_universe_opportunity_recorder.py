#!/usr/bin/env python3
"""Append bounded universe-opportunity rankings to a JSONL archive."""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_universe_opportunity_scan import fetch, rank_candidates  # noqa: E402


def observe(base_url, exchange, market, interval, symbols, limit,
            min_quote_volume, min_realized_vol, min_abs_funding_hourly_pct,
            min_score, timeout):
    symbol_filter = {item.strip().upper() for item in symbols.split(",") if item.strip()}
    symbols_param = ",".join(sorted(symbol_filter)) or None
    common = {
        "exchange": exchange,
        "market": market,
        "symbols": symbols_param,
        "interval": interval,
        "limit": limit,
    }
    volume_payload = fetch(base_url, "/v1/universe/top-volume", common, timeout)
    volatility_payload = fetch(base_url, "/v1/universe/volatility", common, timeout)
    funding_payload = fetch(base_url, "/v1/market/perpetual-funding", {
        "symbols": symbols_param,
        "exchanges": exchange,
        "active_only": "true",
        "limit": 500,
    }, timeout)
    candidates = rank_candidates(
        volume_payload, volatility_payload, funding_payload, exchange,
        min_quote_volume, min_realized_vol, min_abs_funding_hourly_pct,
        min_score, symbol_filter or None,
    )
    return {
        "market": {"exchange": exchange, "market": market, "interval": interval},
        "parameters": {
            "symbols": sorted(symbol_filter),
            "min_quote_volume": min_quote_volume,
            "min_realized_vol": min_realized_vol,
            "min_abs_funding_hourly_pct": min_abs_funding_hourly_pct,
            "min_score": min_score,
        },
        "candidate_count": len(candidates),
        "candidates": candidates,
        "evidence": [
            "volume_universe_available" if volume_payload.get("rows") else "missing_volume_universe",
            "volatility_universe_available" if volatility_payload.get("rows") else "missing_volatility_universe",
            "funding_rows_available" if funding_payload.get("funding") else "missing_funding_rows",
        ],
        "upstream_errors": funding_payload.get("errors", []),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--symbols", default="")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--min-quote-volume", type=float, default=1_000_000.0)
    parser.add_argument("--min-realized-vol", type=float, default=0.0)
    parser.add_argument("--min-abs-funding-hourly-pct", type=float, default=0.01)
    parser.add_argument("--min-score", type=int, default=2)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, default=Path("work/crypto-universe-opportunities.jsonl"))
    options = parser.parse_args()
    if (options.limit <= 0 or options.min_quote_volume < 0 or options.min_realized_vol < 0
            or options.min_abs_funding_hourly_pct < 0 or not 0 <= options.min_score <= 3
            or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid limits, thresholds, score, iteration or interval arguments")

    options.output.parent.mkdir(parents=True, exist_ok=True)
    with options.output.open("a", encoding="utf-8") as handle:
        for iteration in range(options.iterations):
            observation = observe(
                options.base_url, options.exchange, options.market, options.interval,
                options.symbols, options.limit, options.min_quote_volume,
                options.min_realized_vol, options.min_abs_funding_hourly_pct,
                options.min_score, options.timeout,
            )
            handle.write(json.dumps({
                "recorded_at_ms": int(time.time() * 1000),
                "iteration": iteration + 1,
                "observation": observation,
            }, ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
            if iteration + 1 < options.iterations:
                time.sleep(options.interval_secs)
    print(json.dumps({
        "strategy": "crypto_universe_opportunity_recorder",
        "output": str(options.output),
        "appended_snapshots": options.iterations,
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
