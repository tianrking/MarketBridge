#!/usr/bin/env python3
"""Replay an altcoin breadth versus BTC relative-performance hypothesis.

For a caller-selected basket, breadth is the fraction of altcoins whose
trailing return beats BTC over the same lookback.  The replay compares the
equal-weight altcoin basket's next-window return with BTC's return by breadth
state.  It is an explicit proxy for an alt-season index, not the official
market-cap-weighted Top-50 methodology and never allocates or executes.
"""

import argparse
import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from crypto_cross_asset_momentum_replay import aligned_points, candle_points, fetch, load_series


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def breadth_observation(aligned, index, btc_symbol, lookback_bars, horizon_bars,
                        low_threshold, high_threshold, min_alt_assets):
    if (index < lookback_bars or index + horizon_bars >= len(aligned)
            or lookback_bars <= 0 or horizon_bars <= 0):
        return None
    _, current = aligned[index]
    _, trailing = aligned[index - lookback_bars]
    _, future = aligned[index + horizon_bars]
    btc_trailing = forward_return_pct(trailing.get(btc_symbol), current.get(btc_symbol))
    btc_forward = forward_return_pct(current.get(btc_symbol), future.get(btc_symbol))
    if btc_trailing is None or btc_forward is None:
        return None
    alt_symbols = sorted(symbol for symbol in current if symbol != btc_symbol)
    trailing_returns = {
        symbol: forward_return_pct(trailing.get(symbol), current.get(symbol))
        for symbol in alt_symbols
    }
    trailing_returns = {symbol: value for symbol, value in trailing_returns.items()
                        if value is not None}
    if len(trailing_returns) < min_alt_assets:
        return None
    outperformers = [symbol for symbol, value in trailing_returns.items() if value > btc_trailing]
    breadth = len(outperformers) / len(trailing_returns)
    state = ("high_altcoin_breadth" if breadth >= high_threshold else
             "low_altcoin_breadth" if breadth <= low_threshold else "neutral_altcoin_breadth")
    alt_forward = {
        symbol: forward_return_pct(current.get(symbol), future.get(symbol))
        for symbol in trailing_returns
    }
    alt_forward = {symbol: value for symbol, value in alt_forward.items() if value is not None}
    if len(alt_forward) < min_alt_assets:
        return None
    basket_forward = statistics.mean(alt_forward.values())
    relative_edge_bps = (basket_forward - btc_forward) * 100.0
    return {
        "ts_ms": aligned[index][0],
        "forward_ts_ms": aligned[index + horizon_bars][0],
        "btc_symbol": btc_symbol,
        "breadth_state": state,
        "breadth_fraction": breadth,
        "breadth_pct": breadth * 100.0,
        "alt_assets": sorted(trailing_returns),
        "outperforming_assets": outperformers,
        "trailing_btc_return_pct": btc_trailing,
        "trailing_alt_returns_pct": trailing_returns,
        "forward_btc_return_pct": btc_forward,
        "forward_alt_returns_pct": alt_forward,
        "equal_weight_alt_forward_return_pct": basket_forward,
        "relative_edge_bps": relative_edge_bps,
    }


def _state_stats(observations, state, paper_cost_bps):
    values = [row["relative_edge_bps"] for row in observations if row["breadth_state"] == state]
    adjusted = [value - paper_cost_bps for value in values]
    return {
        "observations": len(values),
        "mean_relative_edge_bps": statistics.mean(values) if values else None,
        "median_relative_edge_bps": statistics.median(values) if values else None,
        "mean_cost_adjusted_edge_bps": statistics.mean(adjusted) if adjusted else None,
        "positive_edge_hit_rate": sum(value > 0 for value in adjusted) / len(adjusted)
        if adjusted else None,
    }


def evaluate_breadth(series, btc_symbol, lookback_bars, horizon_bars,
                     low_threshold, high_threshold, min_alt_assets,
                     min_observations, paper_cost_bps, include_observations=False):
    aligned = aligned_points(series)
    observations = [
        item for index in range(len(aligned))
        if (item := breadth_observation(
            aligned, index, btc_symbol, lookback_bars, horizon_bars,
            low_threshold, high_threshold, min_alt_assets,
        )) is not None
    ]
    by_state = {
        state: _state_stats(observations, state, paper_cost_bps)
        for state in ("high_altcoin_breadth", "neutral_altcoin_breadth", "low_altcoin_breadth")
    }
    high_mean = by_state["high_altcoin_breadth"]["mean_cost_adjusted_edge_bps"]
    low_mean = by_state["low_altcoin_breadth"]["mean_cost_adjusted_edge_bps"]
    enough = len(observations) >= min_observations
    return {
        "assets": sorted(str(symbol).upper() for symbol in series),
        "btc_symbol": btc_symbol,
        "aligned_points": len(aligned),
        "observations": len(observations),
        "by_state": by_state,
        "high_minus_low_cost_adjusted_edge_bps": high_mean - low_mean
        if high_mean is not None and low_mean is not None else None,
        "paper_cost_bps": paper_cost_bps,
        "verdict": "altcoin_breadth_response_reported" if enough
        else "observe_only_insufficient_breadth_observations",
        "evidence": [
            "exact_timestamp_intersection",
            "trailing_altcoin_vs_btc_breadth_available" if observations
            else "missing_breadth_or_forward_window",
            "official_altcoin_index_is_not_reproduced",
        ],
        "limitations": [
            "caller-selected symbols are not the official BlockchainCenter Top-50 market-cap universe",
            "breadth is equal-count and trailing-close based, not market-cap weighted or intraday breadth",
            "relative close-to-close response is descriptive and not an allocation, hedge or fill model",
            "paper cost is a sensitivity hurdle, not fees, funding, borrow, slippage or execution",
            "missing history is excluded from exact intersections rather than filled with zero",
        ],
        "research_only": True,
        **({"observations_detail": observations} if include_observations else {}),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--input", help="JSON file containing {symbol: {candles: [...]}}")
    parser.add_argument("--btc-symbol", default="BTCUSDT")
    parser.add_argument("--alt-symbols", default="ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT,ADAUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1d")
    parser.add_argument("--limit", type=int, default=240)
    parser.add_argument("--lookback-bars", type=int, default=90)
    parser.add_argument("--horizon-bars", type=int, default=7)
    parser.add_argument("--low-threshold", type=float, default=0.25)
    parser.add_argument("--high-threshold", type=float, default=0.75)
    parser.add_argument("--min-alt-assets", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--include-observations", action="store_true")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    btc_symbol = args.btc_symbol.strip().upper()
    alt_symbols = [item.strip().upper() for item in args.alt_symbols.split(",") if item.strip()]
    symbols = list(dict.fromkeys([btc_symbol, *alt_symbols]))
    if (len(symbols) < 3 or args.limit <= 0 or args.lookback_bars <= 0
            or args.horizon_bars <= 0 or not 0 <= args.low_threshold < args.high_threshold <= 1
            or args.min_alt_assets <= 0 or args.min_alt_assets > len(symbols) - 1
            or args.min_observations <= 0 or args.paper_cost_bps < 0 or args.timeout <= 0):
        parser.error("invalid symbols, breadth thresholds, windows, cost or observation arguments")
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
    result = evaluate_breadth(
        series, btc_symbol, args.lookback_bars, args.horizon_bars,
        args.low_threshold, args.high_threshold, args.min_alt_assets,
        args.min_observations, args.paper_cost_bps, args.include_observations,
    )
    result.update({
        "strategy": "crypto_altcoin_breadth_replay",
        "parameters": {
            "exchange": args.exchange, "market": args.market, "interval": args.interval,
            "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
            "low_threshold": args.low_threshold, "high_threshold": args.high_threshold,
            "min_alt_assets": args.min_alt_assets, "min_observations": args.min_observations,
            "paper_cost_bps": args.paper_cost_bps,
        },
        "upstream_errors": errors,
        "execution": "research_only_no_orders",
    })
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
