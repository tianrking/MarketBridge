#!/usr/bin/env python3
"""Compare BTC/ETH relative responses after a Deribit volatility spread.

The falsifiable hypothesis is that ETH volatility-index premium versus BTC, or
the reverse, may coincide with a different later ETH-minus-BTC response than
an aligned volatility state.  This is a cross-asset context study, not a
dispersion trade, hedge, option PnL or execution model.
"""

import argparse
import bisect
import json
import statistics
import time

from crypto_deribit_volatility_index_response_replay import number, volatility_points
from crypto_funding_spread_response_replay import fetch, price_points


def align_volatility_points(btc_points, eth_points):
    btc = dict(btc_points)
    eth = dict(eth_points)
    return [(timestamp, btc[timestamp], eth[timestamp])
            for timestamp in sorted(set(btc) & set(eth))]


def forward_return(prices, timestamp, horizon_bars):
    if not prices or horizon_bars <= 0:
        return None
    timestamps = [point[0] for point in prices]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if future_index >= len(prices):
        return None
    baseline = prices[index][1]
    future = prices[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def classify_spread(spread, threshold):
    if spread is None:
        return "observe_only_missing_volatility_spread"
    if spread >= threshold:
        return "eth_volatility_premium_to_btc"
    if spread <= -threshold:
        return "btc_volatility_premium_to_eth"
    return "cross_asset_volatility_aligned"


def build_observations(volatility, btc_prices, eth_prices, horizon_bars, threshold):
    rows = []
    for timestamp, btc_index, eth_index in volatility:
        spread = eth_index - btc_index
        btc_return = forward_return(btc_prices, timestamp, horizon_bars)
        eth_return = forward_return(eth_prices, timestamp, horizon_bars)
        relative = eth_return - btc_return if btc_return is not None and eth_return is not None else None
        rows.append({
            "ts_ms": timestamp,
            "btc_volatility_index_close": btc_index,
            "eth_volatility_index_close": eth_index,
            "eth_minus_btc_volatility_index": spread,
            "state": classify_spread(spread, threshold),
            "btc_forward_return_pct": btc_return,
            "eth_forward_return_pct": eth_return,
            "eth_minus_btc_forward_return_pct": relative,
            "absolute_relative_response_pct": abs(relative) if relative is not None else None,
        })
    return rows


def state_stats(rows, state):
    selected = [row for row in rows
                if row.get("state") == state and row.get("eth_minus_btc_forward_return_pct") is not None]
    relative = [row["eth_minus_btc_forward_return_pct"] for row in selected]
    absolute = [row["absolute_relative_response_pct"] for row in selected]
    return {
        "observations": len(selected),
        "mean_eth_minus_btc_forward_return_pct": statistics.mean(relative) if relative else None,
        "mean_absolute_relative_response_pct": statistics.mean(absolute) if absolute else None,
    }


def summarize(rows, min_observations, min_edge_bps):
    states = (
        "eth_volatility_premium_to_btc",
        "cross_asset_volatility_aligned",
        "btc_volatility_premium_to_eth",
    )
    buckets = {state: state_stats(rows, state) for state in states}
    aligned = buckets["cross_asset_volatility_aligned"]["mean_absolute_relative_response_pct"]
    eth_premium = buckets["eth_volatility_premium_to_btc"]["mean_absolute_relative_response_pct"]
    btc_premium = buckets["btc_volatility_premium_to_eth"]["mean_absolute_relative_response_pct"]
    eth_edge = ((eth_premium - aligned) * 100.0
                if eth_premium is not None and aligned is not None else None)
    btc_edge = ((btc_premium - aligned) * 100.0
                if btc_premium is not None and aligned is not None else None)
    verdict = "observe_only"
    if (buckets["eth_volatility_premium_to_btc"]["observations"] >= min_observations
            and eth_edge is not None and eth_edge >= min_edge_bps):
        verdict = "eth_volatility_premium_relative_response_candidate"
    elif (buckets["btc_volatility_premium_to_eth"]["observations"] >= min_observations
          and btc_edge is not None and btc_edge >= min_edge_bps):
        verdict = "btc_volatility_premium_relative_response_candidate"
    return {
        "by_state": buckets,
        "eth_premium_minus_aligned_absolute_relative_edge_bps": eth_edge,
        "btc_premium_minus_aligned_absolute_relative_edge_bps": btc_edge,
        "min_edge_bps": min_edge_bps,
        "verdict": verdict,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--resolution", default="3600", choices=("1", "60", "3600", "43200", "1D"))
    parser.add_argument("--days", type=float, default=30.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--btc-symbol", default="BTCUSDT")
    parser.add_argument("--eth-symbol", default="ETHUSDT")
    parser.add_argument("--price-interval", default="1h")
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--spread-threshold", type=float, default=5.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.horizon_bars <= 0 or args.spread_threshold < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, horizon, spread, edge, observation or timeout argument")

    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    request = {"resolution": args.resolution, "start_ms": start_ms,
               "end_ms": now_ms, "limit": 1_000}
    btc_volatility_payload = fetch(args.base_url, "/v1/history/volatility-index",
                                   {"currency": "BTC", **request}, args.timeout)
    eth_volatility_payload = fetch(args.base_url, "/v1/history/volatility-index",
                                   {"currency": "ETH", **request}, args.timeout)
    price_params = {
        "exchange": args.price_exchange, "candle_type": "perp",
        "interval": args.price_interval, "start_ms": start_ms,
        "end_ms": now_ms, "limit": 1_500,
    }
    btc_price_payload = fetch(args.base_url, "/v1/history/candles",
                              {"symbol": args.btc_symbol, **price_params}, args.timeout)
    eth_price_payload = fetch(args.base_url, "/v1/history/candles",
                              {"symbol": args.eth_symbol, **price_params}, args.timeout)
    volatility = align_volatility_points(
        volatility_points(btc_volatility_payload), volatility_points(eth_volatility_payload),
    )
    btc_prices = price_points(btc_price_payload)
    eth_prices = price_points(eth_price_payload)
    observations = build_observations(
        volatility, btc_prices, eth_prices, args.horizon_bars, args.spread_threshold,
    )
    payloads = (("btc_volatility_index", btc_volatility_payload),
                ("eth_volatility_index", eth_volatility_payload),
                ("btc_price", btc_price_payload), ("eth_price", eth_price_payload))
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in payloads if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_deribit_cross_asset_volatility_response_replay",
        "volatility_exchange": "deribit",
        "price_exchange": args.price_exchange,
        "btc_symbol": args.btc_symbol,
        "eth_symbol": args.eth_symbol,
        "resolution": args.resolution,
        "price_interval": args.price_interval,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"spread_threshold": args.spread_threshold,
                    "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations},
        "source_counts": {"aligned_volatility": len(volatility),
                           "btc_price_bars": len(btc_prices), "eth_price_bars": len(eth_prices)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations, args.min_edge_bps),
        "coverage": {
            "btc_volatility_index": btc_volatility_payload.get("coverage_detail"),
            "eth_volatility_index": eth_volatility_payload.get("coverage_detail"),
            "btc_price": btc_price_payload.get("coverage_detail"),
            "eth_price": eth_price_payload.get("coverage_detail"),
        },
        "upstream_errors": errors,
        "limitations": [
            "BTC and ETH provider volatility indexes are not option-chain surfaces or executable quotes",
            "relative response is a descriptive cross-asset comparison and does not establish causality",
            "no dispersion spread, hedge ratio, option PnL, fees, borrow, slippage or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
