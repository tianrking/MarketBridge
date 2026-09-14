#!/usr/bin/env python3
"""Replay a cross-asset crypto momentum/rotation hypothesis.

The hypothesis is deliberately falsifiable: at a fixed rebalance cadence, the
assets with the strongest trailing return should beat an equal-weight benchmark
over the next holding window.  This is a gross, close-to-close observation
only.  It does not model orders, fills, leverage, funding, borrow, fees or
portfolio custody.

MarketBridge remains the data interface.  The script accepts normalized candle
payloads from ``/v1/history/candles`` and keeps exact timestamp intersections;
it never forward-fills a missing asset or turns a missing candle into zero.
"""

import argparse
import json
import statistics
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/history/candles?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) else None


def candle_points(payload):
    """Return de-duplicated ``(timestamp_ms, close)`` points from a payload."""
    rows = payload.get("candles", []) if isinstance(payload, dict) else []
    points = {}
    for row in rows:
        if not isinstance(row, dict):
            continue
        timestamp = row.get("open_time_ms", row.get("ts_ms"))
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points[timestamp] = close
    return sorted(points.items())


def aligned_points(series):
    """Intersect timestamps across assets without fabricating missing prices."""
    cleaned = {
        str(symbol).upper(): dict(points)
        for symbol, points in series.items()
        if points
    }
    if not cleaned:
        return []
    common = set.intersection(*(set(points) for points in cleaned.values()))
    return [
        (timestamp, {symbol: cleaned[symbol][timestamp] for symbol in sorted(cleaned)})
        for timestamp in sorted(common)
    ]


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def momentum_observation(aligned, index, lookback_bars, horizon_bars, top_k):
    if (index < lookback_bars or index + horizon_bars >= len(aligned)
            or lookback_bars <= 0 or horizon_bars <= 0 or top_k <= 0):
        return None
    _, current = aligned[index]
    _, trailing = aligned[index - lookback_bars]
    _, future = aligned[index + horizon_bars]
    trailing_returns = {
        symbol: forward_return_pct(trailing.get(symbol), current.get(symbol))
        for symbol in current
    }
    trailing_returns = {
        symbol: value for symbol, value in trailing_returns.items() if value is not None
    }
    if len(trailing_returns) < top_k:
        return None
    ranked = sorted(trailing_returns.items(), key=lambda item: (-item[1], item[0]))
    selected = [symbol for symbol, _ in ranked[:top_k]]
    forward_returns = {
        symbol: forward_return_pct(current.get(symbol), future.get(symbol))
        for symbol in current
    }
    forward_returns = {
        symbol: value for symbol, value in forward_returns.items() if value is not None
    }
    if len(forward_returns) < top_k:
        return None
    basket = [forward_returns[symbol] for symbol in selected if symbol in forward_returns]
    if len(basket) != top_k:
        return None
    benchmark = list(forward_returns.values())
    basket_return = statistics.mean(basket)
    benchmark_return = statistics.mean(benchmark)
    result = {
        "ts_ms": aligned[index][0],
        "forward_ts_ms": aligned[index + horizon_bars][0],
        "selected_symbols": selected,
        "trailing_returns_pct": trailing_returns,
        "forward_returns_pct": forward_returns,
        "basket_forward_return_pct": basket_return,
        "equal_weight_forward_return_pct": benchmark_return,
        "edge_bps": (basket_return - benchmark_return) * 100.0,
    }
    return result


def evaluate_momentum(series, lookback_bars, horizon_bars, top_k,
                      min_edge_bps=0.0, min_observations=1,
                      include_observations=False):
    aligned = aligned_points(series)
    requested_assets = sorted(str(symbol).upper() for symbol in series)
    assets_with_data = sorted(str(symbol).upper() for symbol, points in series.items() if points)
    missing_assets = [symbol for symbol in requested_assets if symbol not in assets_with_data]
    observations = [
        item for index in range(len(aligned))
        if (item := momentum_observation(aligned, index, lookback_bars,
                                         horizon_bars, top_k)) is not None
    ]
    edges = [item["edge_bps"] for item in observations]
    qualifying = [edge for edge in edges if edge >= min_edge_bps]
    mean_edge = statistics.mean(edges) if edges else None
    median_edge = statistics.median(edges) if edges else None
    hit_rate = (sum(edge > 0 for edge in edges) / len(edges)) if edges else None
    enough = len(observations) >= min_observations
    candidate = enough and mean_edge is not None and mean_edge >= min_edge_bps
    result = {
        "assets": requested_assets,
        "assets_with_data": assets_with_data,
        "missing_assets": missing_assets,
        "aligned_points": len(aligned),
        "observations": len(observations),
        "qualifying_observations": len(qualifying),
        "mean_edge_bps": mean_edge,
        "median_edge_bps": median_edge,
        "positive_edge_hit_rate": hit_rate,
        "latest": observations[-1] if observations else None,
        "verdict": "momentum_candidate" if candidate else "observe_only",
        "evidence": [
            "exact_timestamp_intersection",
            "missing_asset_history" if missing_assets else "all_requested_asset_histories_available",
            "cross_asset_forward_window_available" if observations else "missing_cross_asset_window",
            "mean_top_basket_edge_above_threshold" if candidate else "mean_edge_below_threshold_or_insufficient_observations",
        ],
        "limitations": [
            "close-to-close returns are not executable fills",
            "no fees, funding, borrow, slippage, turnover, weight drift or leverage model",
            "equal-weight benchmark and top-k selection are research proxies",
            "a candidate is not a forecast or an order instruction",
        ],
    }
    if include_observations:
        result["observations_detail"] = observations
    return result


def load_series(path):
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    if isinstance(payload, dict) and isinstance(payload.get("series"), dict):
        payload = payload["series"]
    return {
        str(symbol).upper(): candle_points(value)
        for symbol, value in payload.items()
        if isinstance(value, dict)
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
    parser.add_argument("--horizon-bars", type=int, default=8)
    parser.add_argument("--top-k", type=int, default=1)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--include-observations", action="store_true",
                        help="include every replay row instead of only the latest row")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if (len(set(symbols)) < 2 or args.limit <= 0 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or args.top_k <= 0
            or args.top_k > len(set(symbols)) or args.min_edge_bps < 0
            or args.min_observations <= 0):
        parser.error("invalid symbols, windows, top-k, edge or observation arguments")
    if args.input:
        series = load_series(args.input)
    else:
        series = {}
        errors = []
        for symbol in symbols:
            payload = fetch(args.base_url, {
                "exchange": args.exchange,
                "market": args.market,
                "symbol": symbol,
                "interval": args.interval,
                "limit": args.limit,
            }, args.timeout)
            series[symbol] = candle_points(payload)
            if payload.get("error"):
                errors.append({"symbol": symbol, "error": payload["error"]})
    result = evaluate_momentum(
        series, args.lookback_bars, args.horizon_bars, args.top_k,
        args.min_edge_bps, args.min_observations, args.include_observations,
    )
    result.update({
        "strategy": "crypto_cross_asset_momentum",
        "parameters": {
            "exchange": args.exchange,
            "market": args.market,
            "interval": args.interval,
            "lookback_bars": args.lookback_bars,
            "horizon_bars": args.horizon_bars,
            "top_k": args.top_k,
            "min_edge_bps": args.min_edge_bps,
            "min_observations": args.min_observations,
        },
        "upstream_errors": errors if not args.input else [],
        "execution": "research_only_no_orders",
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
