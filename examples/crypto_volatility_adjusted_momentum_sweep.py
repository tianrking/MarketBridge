#!/usr/bin/env python3
"""Run a bounded parameter grid over volatility-adjusted momentum replay.

The sweep exists to expose sensitivity, not to select a live parameter. It
fetches each symbol once, evaluates every requested lookback/volatility/horizon
combination on the same sample, and labels the output as in-sample research.
Any parameter choice still needs a time-held-out replay before it can be
considered evidence.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_cross_asset_momentum_replay import candle_points, load_series
from crypto_volatility_adjusted_momentum_replay import (
    evaluate_volatility_adjusted_momentum,
)


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/history/candles?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def parse_int_grid(value, name):
    try:
        values = [int(item.strip()) for item in value.split(",") if item.strip()]
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"{name} must be comma-separated positive integers") from error
    if not values or any(item <= 0 for item in values):
        raise argparse.ArgumentTypeError(f"{name} must contain positive integers")
    return sorted(set(values))


def run_grid(series, lookbacks, volatility_windows, horizons, top_k,
             min_edge_bps, min_observations, roundtrip_cost_bps=0.0):
    rows = []
    for lookback in lookbacks:
        for volatility in volatility_windows:
            for horizon in horizons:
                result = evaluate_volatility_adjusted_momentum(
                    series, lookback, horizon, volatility, top_k,
                    min_edge_bps, min_observations,
                    roundtrip_cost_bps=roundtrip_cost_bps,
                )
                rows.append({
                    "lookback_bars": lookback,
                    "volatility_bars": volatility,
                    "horizon_bars": horizon,
                    "observations": result["observations"],
                    "mean_edge_bps": result["mean_edge_bps"],
                    "median_edge_bps": result["median_edge_bps"],
                    "paper_cost_bps": result["paper_cost_bps"],
                    "mean_cost_adjusted_edge_bps": result["mean_cost_adjusted_edge_bps"],
                    "median_cost_adjusted_edge_bps": result["median_cost_adjusted_edge_bps"],
                    "positive_edge_hit_rate": result["positive_edge_hit_rate"],
                    "cost_adjusted_positive_edge_hit_rate": result["cost_adjusted_positive_edge_hit_rate"],
                    "verdict": result["verdict"],
                    "evidence": result["evidence"],
                })
    ranked = sorted(rows, key=lambda row: (
        row["mean_edge_bps"] is None,
        -(row["mean_edge_bps"] or 0.0),
        -row["observations"],
    ))
    return {
        "grid_size": len(rows),
        "rows": ranked,
        "best_in_sample": ranked[0] if ranked else None,
        "selection_warning": (
            "best_in_sample is descriptive only; do not treat it as validated without "
            "time-held-out data and cost-aware paper replay"
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--input", help="JSON file containing {symbol: {candles: [...]}}")
    parser.add_argument("--symbols", default="BTCUSDT,ETHUSDT,SOLUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--limit", type=int, default=240)
    parser.add_argument("--lookback-bars", type=lambda value: parse_int_grid(value, "lookback-bars"), default=[8, 12, 24])
    parser.add_argument("--volatility-bars", type=lambda value: parse_int_grid(value, "volatility-bars"), default=[8, 12, 24])
    parser.add_argument("--horizon-bars", type=lambda value: parse_int_grid(value, "horizon-bars"), default=[4, 8, 12])
    parser.add_argument("--top-k", type=int, default=1)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--roundtrip-cost-bps", type=float, default=0.0,
                        help="paper hurdle subtracted from each relative edge")
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if (len(set(symbols)) < 2 or args.limit <= 0 or args.top_k <= 0
            or args.top_k > len(set(symbols)) or args.min_edge_bps < 0
            or args.roundtrip_cost_bps < 0
            or args.min_observations <= 0):
        parser.error("invalid symbols, limit, top-k, edge or observation arguments")
    if args.input:
        series = load_series(args.input)
        errors = []
    else:
        series = {}
        errors = []
        for symbol in symbols:
            payload = fetch(args.base_url, {
                "exchange": args.exchange, "market": args.market,
                "symbol": symbol, "interval": args.interval, "limit": args.limit,
            }, args.timeout)
            series[symbol] = candle_points(payload)
            if payload.get("error"):
                errors.append({"symbol": symbol, "error": payload["error"]})
    grid = run_grid(
        series, args.lookback_bars, args.volatility_bars, args.horizon_bars,
        args.top_k, args.min_edge_bps, args.min_observations,
        args.roundtrip_cost_bps,
    )
    print(json.dumps({
        "strategy": "crypto_volatility_adjusted_momentum_sweep",
        "parameters": {
            "exchange": args.exchange, "market": args.market, "interval": args.interval,
            "symbols": symbols, "limit": args.limit, "top_k": args.top_k,
            "lookback_bars": args.lookback_bars,
            "volatility_bars": args.volatility_bars,
            "horizon_bars": args.horizon_bars,
            "min_edge_bps": args.min_edge_bps,
            "min_observations": args.min_observations,
            "roundtrip_cost_bps": args.roundtrip_cost_bps,
        },
        "assets_with_data": sorted(symbol for symbol, points in series.items() if points),
        "missing_assets": sorted(symbol for symbol in symbols if not series.get(symbol)),
        "upstream_errors": errors,
        "grid": grid,
        "limitations": [
            "all rows reuse one sample and are in-sample comparisons",
            "no multiple-testing correction or time-held-out validation is claimed",
            "roundtrip_cost_bps is a relative paper hurdle, not a fill or venue-fee model",
            "no funding, borrow, turnover, weight drift or leverage model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
