#!/usr/bin/env python3
"""Replay volatility-adjusted cross-asset crypto momentum.

The falsifiable hypothesis is deliberately distinct from raw momentum: rank
assets by trailing return divided by their trailing realized volatility, then
compare the selected basket's next-window return with an equal-weight basket.
This is a close-to-close research replay, not a portfolio allocator. An
optional round-trip cost hurdle can be applied to the relative edge; it is a
paper sensitivity, not a fill model.
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


def volatility_adjusted_observation(aligned, index, lookback_bars, horizon_bars,
                                    volatility_bars, top_k, roundtrip_cost_bps=0.0):
    if (index < max(lookback_bars, volatility_bars)
            or index + horizon_bars >= len(aligned)
            or lookback_bars <= 0 or horizon_bars <= 0
            or volatility_bars <= 1 or top_k <= 0):
        return None
    _, current = aligned[index]
    _, trailing = aligned[index - lookback_bars]
    _, future = aligned[index + horizon_bars]
    scores = {}
    trailing_returns = {}
    trailing_volatility = {}
    for symbol, current_price in current.items():
        start_price = trailing.get(symbol)
        trailing_return = forward_return_pct(start_price, current_price)
        window_start = index - volatility_bars
        volatility_values = [aligned[offset][1].get(symbol)
                             for offset in range(window_start, index + 1)]
        volatility = realized_vol_pct(volatility_values)
        if trailing_return is None or volatility is None or volatility <= 0:
            continue
        trailing_returns[symbol] = trailing_return
        trailing_volatility[symbol] = volatility
        scores[symbol] = trailing_return / volatility
    if len(scores) < top_k:
        return None
    selected = [symbol for symbol, _ in sorted(scores.items(), key=lambda item: (-item[1], item[0]))[:top_k]]
    forward_returns = {
        symbol: forward_return_pct(current.get(symbol), future.get(symbol))
        for symbol in current
        if forward_return_pct(current.get(symbol), future.get(symbol)) is not None
    }
    basket = [forward_returns[symbol] for symbol in selected if symbol in forward_returns]
    if len(basket) != top_k or not forward_returns:
        return None
    basket_return = statistics.mean(basket)
    benchmark_return = statistics.mean(forward_returns.values())
    gross_edge_bps = (basket_return - benchmark_return) * 100.0
    return {
        "ts_ms": aligned[index][0],
        "forward_ts_ms": aligned[index + horizon_bars][0],
        "selected_symbols": selected,
        "risk_adjusted_scores": scores,
        "trailing_returns_pct": trailing_returns,
        "trailing_volatility_pct_per_bar": trailing_volatility,
        "forward_returns_pct": forward_returns,
        "basket_forward_return_pct": basket_return,
        "equal_weight_forward_return_pct": benchmark_return,
        "gross_edge_bps": gross_edge_bps,
        "paper_cost_bps": roundtrip_cost_bps,
        "cost_adjusted_edge_bps": gross_edge_bps - roundtrip_cost_bps,
        "edge_bps": gross_edge_bps,
    }


def evaluate_volatility_adjusted_momentum(series, lookback_bars, horizon_bars,
                                          volatility_bars, top_k, min_edge_bps=0.0,
                                          min_observations=1, include_observations=False,
                                          roundtrip_cost_bps=0.0):
    aligned = aligned_points(series)
    requested_assets = sorted(str(symbol).upper() for symbol in series)
    assets_with_data = sorted(str(symbol).upper() for symbol, points in series.items() if points)
    observations = [
        item for index in range(len(aligned))
        if (item := volatility_adjusted_observation(
            aligned, index, lookback_bars, horizon_bars, volatility_bars, top_k,
            roundtrip_cost_bps,
        )) is not None
    ]
    edges = [item["edge_bps"] for item in observations]
    cost_adjusted_edges = [item["cost_adjusted_edge_bps"] for item in observations]
    mean_edge = statistics.mean(edges) if edges else None
    mean_cost_adjusted_edge = statistics.mean(cost_adjusted_edges) if cost_adjusted_edges else None
    enough = len(observations) >= min_observations
    candidate = (enough and mean_cost_adjusted_edge is not None
                 and mean_cost_adjusted_edge >= min_edge_bps)
    result = {
        "assets": requested_assets,
        "assets_with_data": assets_with_data,
        "missing_assets": [symbol for symbol in requested_assets if symbol not in assets_with_data],
        "aligned_points": len(aligned),
        "observations": len(observations),
        "qualifying_observations": sum(edge >= min_edge_bps for edge in cost_adjusted_edges),
        "mean_edge_bps": mean_edge,
        "median_edge_bps": statistics.median(edges) if edges else None,
        "positive_edge_hit_rate": (sum(edge > 0 for edge in edges) / len(edges)) if edges else None,
        "paper_cost_bps": roundtrip_cost_bps,
        "mean_cost_adjusted_edge_bps": mean_cost_adjusted_edge,
        "median_cost_adjusted_edge_bps": (
            statistics.median(cost_adjusted_edges) if cost_adjusted_edges else None
        ),
        "cost_adjusted_positive_edge_hit_rate": (
            sum(edge > 0 for edge in cost_adjusted_edges) / len(cost_adjusted_edges)
            if cost_adjusted_edges else None
        ),
        "latest": observations[-1] if observations else None,
        "verdict": "volatility_adjusted_momentum_candidate" if candidate else "observe_only",
        "evidence": [
            "exact_timestamp_intersection",
            "realized_volatility_scaled_ranking",
            "missing_asset_history" if len(assets_with_data) < len(requested_assets)
            else "all_requested_asset_histories_available",
            "cross_asset_forward_window_available" if observations else "missing_cross_asset_window_or_volatility",
            "mean_cost_adjusted_edge_above_threshold"
            if candidate else "mean_edge_below_threshold_or_insufficient_observations",
        ],
        "limitations": [
            "volatility is close-to-close per-bar population dispersion and is not annualized",
            "zero-volatility assets are excluded rather than assigned infinite score",
            "paper_cost_bps is a conservative relative hurdle, not a fill, queue or venue-fee model",
            "no funding, borrow, turnover, weight drift or leverage model",
            "a candidate is not a forecast, allocation or order instruction",
        ],
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
    parser.add_argument("--top-k", type=int, default=1)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--roundtrip-cost-bps", type=float, default=0.0,
                        help="paper hurdle subtracted from each relative edge")
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--include-observations", action="store_true")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if (len(set(symbols)) < 2 or args.limit <= 0 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.volatility_bars <= 1 or args.top_k <= 0
            or args.top_k > len(set(symbols)) or args.min_edge_bps < 0
            or args.roundtrip_cost_bps < 0
            or args.min_observations <= 0):
        parser.error("invalid symbols, windows, top-k, edge or observation arguments")
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
    result = evaluate_volatility_adjusted_momentum(
        series, args.lookback_bars, args.horizon_bars, args.volatility_bars,
        args.top_k, args.min_edge_bps, args.min_observations, args.include_observations,
        args.roundtrip_cost_bps,
    )
    result.update({
        "strategy": "crypto_volatility_adjusted_momentum",
        "parameters": {
            "exchange": args.exchange, "market": args.market, "interval": args.interval,
            "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
            "volatility_bars": args.volatility_bars, "top_k": args.top_k,
            "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations,
            "roundtrip_cost_bps": args.roundtrip_cost_bps,
        },
        "upstream_errors": errors,
        "execution": "research_only_no_orders",
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
