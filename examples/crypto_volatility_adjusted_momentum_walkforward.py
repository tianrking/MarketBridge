#!/usr/bin/env python3
"""Run a time-ordered holdout replay for volatility-adjusted momentum.

This keeps the same normalized candle input as the regular replay, but
separates observations before and after a timestamp split. The warm-up bars
before the split are retained for test features; no post-split prices are used
to form pre-split rankings.
"""

import argparse
import json
import statistics

from crypto_cross_asset_momentum_replay import candle_points, fetch, load_series, aligned_points
from crypto_volatility_adjusted_momentum_replay import volatility_adjusted_observation


def summarize_observations(observations, min_observations):
    edges = [item["cost_adjusted_edge_bps"] for item in observations]
    return {
        "observations": len(observations),
        "mean_cost_adjusted_edge_bps": statistics.mean(edges) if edges else None,
        "median_cost_adjusted_edge_bps": statistics.median(edges) if edges else None,
        "positive_edge_hit_rate": (sum(edge > 0 for edge in edges) / len(edges)) if edges else None,
        "enough_observations": len(observations) >= min_observations,
        "first_ts_ms": observations[0]["ts_ms"] if observations else None,
        "last_ts_ms": observations[-1]["ts_ms"] if observations else None,
    }


def evaluate_walkforward(series, lookback_bars, horizon_bars, volatility_bars,
                         top_k, train_fraction, min_observations,
                         roundtrip_cost_bps=0.0):
    aligned = aligned_points(series)
    split_index = int(len(aligned) * train_fraction) if aligned else 0
    if split_index <= 0 or split_index >= len(aligned):
        return {
            "aligned_points": len(aligned),
            "train": summarize_observations([], min_observations),
            "test": summarize_observations([], min_observations),
            "verdict": "observe_only_invalid_split",
            "evidence": ["invalid_or_insufficient_time_split"],
        }
    split_ts = aligned[split_index][0]
    observations = []
    for index in range(len(aligned)):
        observation = volatility_adjusted_observation(
            aligned, index, lookback_bars, horizon_bars, volatility_bars,
            top_k, roundtrip_cost_bps,
        )
        if observation is not None:
            observation["split"] = "train" if observation["ts_ms"] < split_ts else "test"
            observations.append(observation)
    train = [item for item in observations if item["split"] == "train"]
    test = [item for item in observations if item["split"] == "test"]
    train_summary = summarize_observations(train, min_observations)
    test_summary = summarize_observations(test, min_observations)
    survives = (
        train_summary["enough_observations"] and test_summary["enough_observations"]
        and (train_summary["mean_cost_adjusted_edge_bps"] or 0.0) > 0.0
        and (test_summary["mean_cost_adjusted_edge_bps"] or 0.0) > 0.0
    )
    return {
        "aligned_points": len(aligned),
        "split_index": split_index,
        "split_ts_ms": split_ts,
        "warmup_bars": max(lookback_bars, volatility_bars),
        "train": train_summary,
        "test": test_summary,
        "verdict": "holdout_edge_survives" if survives else "observe_only",
        "evidence": [
            "time_ordered_train_test_split",
            "test_features_use_only_pre_split_warmup_and_post_split_current_data",
            "holdout_cost_adjusted_edge_positive" if survives
            else "holdout_edge_missing_or_not_positive",
        ],
        "observations_detail": observations,
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
    parser.add_argument("--lookback-bars", type=int, default=8)
    parser.add_argument("--volatility-bars", type=int, default=8)
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--top-k", type=int, default=1)
    parser.add_argument("--train-fraction", type=float, default=0.7)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--roundtrip-cost-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if (len(set(symbols)) < 2 or args.limit <= 0 or args.lookback_bars <= 0
            or args.volatility_bars <= 1 or args.horizon_bars <= 0 or args.top_k <= 0
            or args.top_k > len(set(symbols)) or not 0.0 < args.train_fraction < 1.0
            or args.min_observations <= 0 or args.roundtrip_cost_bps < 0):
        parser.error("invalid symbols, windows, split, cost or observation arguments")
    errors = []
    if args.input:
        series = load_series(args.input)
    else:
        series = {}
        for symbol in symbols:
            payload = fetch(args.base_url, {
                "exchange": args.exchange, "market": args.market,
                "symbol": symbol, "interval": args.interval, "limit": args.limit,
            }, args.timeout)
            series[symbol] = candle_points(payload)
            if payload.get("error"):
                errors.append({"symbol": symbol, "error": payload["error"]})
    result = evaluate_walkforward(
        series, args.lookback_bars, args.horizon_bars, args.volatility_bars,
        args.top_k, args.train_fraction, args.min_observations,
        args.roundtrip_cost_bps,
    )
    result.update({
        "strategy": "crypto_volatility_adjusted_momentum_walkforward",
        "parameters": {
            "exchange": args.exchange, "market": args.market, "interval": args.interval,
            "symbols": symbols, "limit": args.limit, "lookback_bars": args.lookback_bars,
            "volatility_bars": args.volatility_bars, "horizon_bars": args.horizon_bars,
            "top_k": args.top_k, "train_fraction": args.train_fraction,
            "min_observations": args.min_observations,
            "roundtrip_cost_bps": args.roundtrip_cost_bps,
        },
        "upstream_errors": errors,
        "limitations": [
            "one chronological split is not a proof of stable out-of-sample alpha",
            "paper cost is a relative hurdle, not a fill, queue or capacity model",
            "no funding, borrow, turnover, weight drift or leverage model",
        ],
        "execution": "research_only_no_orders",
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
