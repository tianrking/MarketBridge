#!/usr/bin/env python3
"""Replay a volatility-normalized, confidence-capped cross-asset paper index.

This is a transparent analogue of public multi-asset, horizon-aligned strategy
descriptions: each asset receives a signed trailing-return/realized-volatility
score, conflicting scores shrink the net exposure toward neutral, and the
result is compared with an equal-weight basket. It uses no external prediction
service and never allocates capital or executes orders.
"""

import argparse
import json
import math
import statistics

from crypto_cross_asset_momentum_replay import (
    aligned_points,
    candle_points,
    fetch,
    forward_return_pct,
    load_series,
)


def realized_vol_pct(values):
    if len(values) < 2 or any(value is None or value <= 0 for value in values):
        return None
    returns = [math.log(current / previous) for previous, current in zip(values, values[1:])]
    return statistics.pstdev(returns) * 100.0


def adaptive_observation(aligned, index, lookback_bars, horizon_bars,
                         volatility_bars, min_net_exposure, max_gross_exposure):
    if (index < max(lookback_bars, volatility_bars)
            or index + horizon_bars >= len(aligned)
            or lookback_bars <= 0 or horizon_bars <= 0 or volatility_bars <= 1
            or not 0 < min_net_exposure <= 1 or not 0 < max_gross_exposure <= 1):
        return None
    _, current = aligned[index]
    _, trailing = aligned[index - lookback_bars]
    _, future = aligned[index + horizon_bars]
    scores, trailing_returns, trailing_volatility = {}, {}, {}
    for symbol, current_price in current.items():
        trailing_return = forward_return_pct(trailing.get(symbol), current_price)
        window = [aligned[offset][1].get(symbol)
                  for offset in range(index - volatility_bars, index + 1)]
        volatility = realized_vol_pct(window)
        if trailing_return is None or volatility is None or volatility <= 0:
            continue
        scores[symbol] = trailing_return / volatility
        trailing_returns[symbol] = trailing_return
        trailing_volatility[symbol] = volatility
    forward_returns = {
        symbol: forward_return_pct(current.get(symbol), future.get(symbol))
        for symbol in scores
        if forward_return_pct(current.get(symbol), future.get(symbol)) is not None
    }
    if len(scores) < 2 or len(forward_returns) != len(scores):
        return None
    denominator = sum(abs(value) for value in scores.values())
    if denominator <= 0:
        return None
    raw_weights = {symbol: value / denominator for symbol, value in scores.items()}
    net_exposure = sum(raw_weights.values())
    state = "neutral" if abs(net_exposure) < min_net_exposure else (
        "net_long" if net_exposure > 0 else "net_short"
    )
    gross_scale = 0.0 if state == "neutral" else max_gross_exposure
    weights = {symbol: weight * gross_scale for symbol, weight in raw_weights.items()}
    adaptive_return = sum(weights[symbol] * forward_returns[symbol] for symbol in weights)
    equal_weight_return = statistics.mean(forward_returns.values())
    edge_bps = (adaptive_return - equal_weight_return) * 100.0
    return {
        "ts_ms": aligned[index][0],
        "forward_ts_ms": aligned[index + horizon_bars][0],
        "state": state,
        "scores": scores,
        "weights": weights,
        "net_exposure": sum(weights.values()),
        "gross_exposure": sum(abs(weight) for weight in weights.values()),
        "trailing_returns_pct": trailing_returns,
        "trailing_volatility_pct_per_bar": trailing_volatility,
        "forward_returns_pct": forward_returns,
        "adaptive_forward_return_pct": adaptive_return,
        "equal_weight_forward_return_pct": equal_weight_return,
        "edge_bps": edge_bps,
    }


def evaluate_adaptive_cross_asset(series, lookback_bars, horizon_bars, volatility_bars,
                                  min_net_exposure=0.10, max_gross_exposure=1.0,
                                  min_observations=1, roundtrip_cost_bps=0.0,
                                  include_observations=False):
    aligned = aligned_points(series)
    assets = sorted(str(symbol).upper() for symbol in series)
    observations = [item for index in range(len(aligned))
                    if (item := adaptive_observation(
                        aligned, index, lookback_bars, horizon_bars, volatility_bars,
                        min_net_exposure, max_gross_exposure,
                    )) is not None]
    adaptive_returns = [item["adaptive_forward_return_pct"] for item in observations]
    benchmark_returns = [item["equal_weight_forward_return_pct"] for item in observations]
    edges = [item["edge_bps"] for item in observations]
    adjusted_edges = [edge - roundtrip_cost_bps for edge in edges]
    mean_edge = statistics.mean(edges) if edges else None
    mean_adjusted = statistics.mean(adjusted_edges) if adjusted_edges else None
    enough = len(observations) >= min_observations
    result = {
        "assets": assets,
        "aligned_points": len(aligned),
        "observations": len(observations),
        "state_counts": {state: sum(item["state"] == state for item in observations)
                         for state in ("net_long", "net_short", "neutral")},
        "mean_adaptive_forward_return_pct": statistics.mean(adaptive_returns) if adaptive_returns else None,
        "mean_equal_weight_forward_return_pct": statistics.mean(benchmark_returns) if benchmark_returns else None,
        "mean_edge_bps": mean_edge,
        "median_edge_bps": statistics.median(edges) if edges else None,
        "positive_edge_hit_rate": (sum(edge > 0 for edge in edges) / len(edges)) if edges else None,
        "paper_cost_bps": roundtrip_cost_bps,
        "mean_cost_adjusted_edge_bps": mean_adjusted,
        "verdict": "adaptive_cross_asset_response_reported" if enough else "observe_only",
        "evidence": [
            "exact_timestamp_intersection",
            "volatility_normalized_signed_scores",
            "conflicting_scores_can_shrink_to_neutral",
            "fixed_forward_window_available" if observations else "missing_cross_asset_window_or_volatility",
        ],
        "limitations": [
            "scores are trailing close-to-close returns divided by per-bar realized volatility",
            "neutral state is a paper exposure rule, not a risk-management or execution engine",
            "no external prediction model, funding, borrow, turnover, fees or slippage model",
            "equal-weight comparison and paper cost are descriptive, not an allocation recommendation",
        ],
        "research_only": True,
    }
    if include_observations:
        result["observations_detail"] = observations
    return result


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
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--volatility-bars", type=int, default=8)
    parser.add_argument("--min-net-exposure", type=float, default=0.10)
    parser.add_argument("--max-gross-exposure", type=float, default=1.0)
    parser.add_argument("--roundtrip-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--include-observations", action="store_true")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if (len(set(symbols)) < 2 or args.limit <= 0 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.volatility_bars <= 1
            or not 0 < args.min_net_exposure <= 1 or not 0 < args.max_gross_exposure <= 1
            or args.roundtrip_cost_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid symbols, windows, exposure, cost or observation arguments")
    if args.input:
        series, errors = load_series(args.input), []
    else:
        series, errors = {}, []
        for symbol in symbols:
            payload = fetch(args.base_url, {
                "exchange": args.exchange, "market": args.market,
                "symbol": symbol, "interval": args.interval, "limit": args.limit,
            }, args.timeout)
            series[symbol] = candle_points(payload)
            if payload.get("error"):
                errors.append({"symbol": symbol, "error": payload["error"]})
    result = evaluate_adaptive_cross_asset(
        series, args.lookback_bars, args.horizon_bars, args.volatility_bars,
        args.min_net_exposure, args.max_gross_exposure, args.min_observations,
        args.roundtrip_cost_bps, args.include_observations,
    )
    result.update({
        "strategy": "crypto_adaptive_cross_asset_replay",
        "parameters": {
            "exchange": args.exchange, "market": args.market, "interval": args.interval,
            "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
            "volatility_bars": args.volatility_bars,
            "min_net_exposure": args.min_net_exposure,
            "max_gross_exposure": args.max_gross_exposure,
            "roundtrip_cost_bps": args.roundtrip_cost_bps,
            "min_observations": args.min_observations,
        },
        "upstream_errors": errors,
        "execution": "research_only_no_orders",
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
