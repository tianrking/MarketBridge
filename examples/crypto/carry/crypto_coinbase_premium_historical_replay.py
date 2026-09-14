#!/usr/bin/env python3
"""Replay historical Coinbase spot-premium states against later BTC returns."""

import argparse
import json
import math
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_coinbase_premium_monitor import classify_premium


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def historical_observations(coinbase, reference, horizon_bars, threshold_bps):
    left, right = dict(coinbase), dict(reference)
    timestamps = sorted(set(left) & set(right))
    rows = []
    for index, timestamp in enumerate(timestamps):
        future_index = index + horizon_bars
        if future_index >= len(timestamps):
            break
        coinbase_price, reference_price = left[timestamp], right[timestamp]
        future_price = right[timestamps[future_index]]
        premium_bps = math.log(coinbase_price / reference_price) * 10_000.0
        forward = (future_price / reference_price - 1.0) * 100.0
        rows.append({
            "ts_ms": timestamp,
            "forward_ts_ms": timestamps[future_index],
            "coinbase_price": coinbase_price,
            "reference_price": reference_price,
            "premium_bps": premium_bps,
            "state": classify_premium(premium_bps, threshold_bps),
            "forward_return_pct": forward,
            "absolute_forward_return_pct": abs(forward),
        })
    return rows


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    absolute = [row["absolute_forward_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
        "median_absolute_forward_return_pct": statistics.median(absolute) if absolute else None,
    }


def summarize(rows, min_observations, min_edge_bps):
    states = ("coinbase_premium", "coinbase_discount", "ordinary_coinbase_reference_spread")
    by_state = {state: bucket_stats([row for row in rows if row["state"] == state])
                for state in states}
    premium = by_state["coinbase_premium"]["mean_absolute_forward_return_pct"]
    ordinary = by_state["ordinary_coinbase_reference_spread"]["mean_absolute_forward_return_pct"]
    edge_bps = ((premium - ordinary) * 100.0
                if premium is not None and ordinary is not None else None)
    candidate = (by_state["coinbase_premium"]["observations"] >= min_observations
                 and edge_bps is not None and edge_bps >= min_edge_bps)
    return {
        "by_state": by_state,
        "aligned_forward_windows": len(rows),
        "premium_minus_ordinary_absolute_move_edge_bps": edge_bps,
        "min_edge_bps": min_edge_bps,
        "verdict": "coinbase_premium_historical_candidate" if candidate else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--coinbase-symbol", default="BTCUSDT")
    parser.add_argument("--reference-symbol", default="BTCUSDT")
    parser.add_argument("--reference-exchange", default="binance")
    parser.add_argument("--interval", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--limit", type=int, default=300)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--premium-threshold-bps", type=float, default=5.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.horizon_bars <= 0
            or args.premium_threshold_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, observation or timeout argument")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"interval": args.interval, "start_ms": start_ms, "end_ms": now_ms,
              "limit": min(args.limit, 300)}
    coinbase_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": "coinbase", "candle_type": "spot",
        "symbol": args.coinbase_symbol,
    }, args.timeout)
    reference_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.reference_exchange, "candle_type": "spot",
        "symbol": args.reference_symbol,
    }, args.timeout)
    coinbase = price_points(coinbase_payload)
    reference = price_points(reference_payload)
    rows = historical_observations(coinbase, reference, args.horizon_bars,
                                    args.premium_threshold_bps)
    print(json.dumps({
        "strategy": "crypto_coinbase_premium_historical_replay",
        "venues": {"coinbase": "coinbase", "reference": args.reference_exchange},
        "symbols": {"coinbase": args.coinbase_symbol, "reference": args.reference_symbol},
        "interval": args.interval,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"horizon_bars": args.horizon_bars,
                    "premium_threshold_bps": args.premium_threshold_bps,
                    "min_edge_bps": args.min_edge_bps,
                    "min_observations": args.min_observations},
        "source_counts": {"coinbase": len(coinbase), "reference": len(reference),
                           "aligned_forward_windows": len(rows)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations, args.min_edge_bps),
        "coverage": {"coinbase": coinbase_payload.get("coverage_detail"),
                     "reference": reference_payload.get("coverage_detail")},
        "upstream_errors": [
            {"source": source, "error": payload["error"]}
            for source, payload in (("coinbase", coinbase_payload),
                                    ("reference", reference_payload))
            if payload.get("error")
        ],
        "limitations": [
            "Coinbase USD and reference USDT quotes can include a stablecoin or FX basis",
            "exact candle intersections are bounded by both providers and may omit no-tick periods",
            "historical response is descriptive; no fees, transfer, inventory, fill or execution model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
