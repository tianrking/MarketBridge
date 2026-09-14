#!/usr/bin/env python3
"""Replay a volume-synchronized order-flow toxicity (VPIN) proxy.

The falsifiable question is narrow: after fixed quote-notional trade buckets
show persistently high absolute signed imbalance, is the next fixed number of
volume buckets followed by a different absolute price move than normal-VPIN
buckets?  This is a non-directional market-stress study, not a prediction,
kill-switch or execution model.
"""

import argparse
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def trade_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        notional = number(row.get("notional"))
        price = number(row.get("price"))
        side = str(row.get("side", "")).lower()
        if (isinstance(ts_ms, int) and notional is not None and notional > 0
                and price is not None and price > 0 and side in {"buy", "sell"}):
            rows.append({"ts_ms": ts_ms, "notional": notional, "price": price, "side": side})
    return sorted(rows, key=lambda row: (row["ts_ms"], row["side"], row["price"]))


def volume_buckets(trades, bucket_notional):
    """Build complete equal-notional buckets; discard a partial final bucket."""
    buckets = []
    start_ts = None
    total = 0.0
    signed = 0.0
    count = 0
    last_ts = None
    last_price = None
    for trade in trades:
        if start_ts is None:
            start_ts = trade["ts_ms"]
        sign = 1.0 if trade["side"] == "buy" else -1.0
        total += trade["notional"]
        signed += sign * trade["notional"]
        count += 1
        last_ts = trade["ts_ms"]
        last_price = trade["price"]
        if total < bucket_notional:
            continue
        buckets.append({
            "start_ts_ms": start_ts,
            "end_ts_ms": last_ts,
            "close_price": last_price,
            "total_notional": total,
            "signed_notional": signed,
            "signed_imbalance_ratio": signed / total,
            "absolute_imbalance_ratio": abs(signed) / total,
            "trade_count": count,
        })
        start_ts = None
        total = 0.0
        signed = 0.0
        count = 0
        last_ts = None
        last_price = None
    return buckets


def vpin_observations(buckets, window_buckets, horizon_buckets, high_vpin):
    observations = []
    if window_buckets <= 0 or horizon_buckets <= 0:
        return observations
    for index in range(window_buckets - 1, len(buckets) - horizon_buckets):
        current = buckets[index]
        future = buckets[index + horizon_buckets]
        current_price = current["close_price"]
        future_price = future["close_price"]
        if current_price is None or future_price is None or current_price <= 0 or future_price <= 0:
            continue
        window = buckets[index - window_buckets + 1:index + 1]
        vpin = statistics.mean(row["absolute_imbalance_ratio"] for row in window)
        forward_return_pct = (future_price / current_price - 1.0) * 100.0
        observations.append({
            "bucket_index": index,
            "ts_ms": current["end_ts_ms"],
            "forward_ts_ms": future["end_ts_ms"],
            "vpin_proxy": vpin,
            "state": "high_vpin" if vpin >= high_vpin else "normal_vpin",
            "forward_return_pct": forward_return_pct,
            "absolute_forward_return_bps": abs(forward_return_pct) * 100.0,
            "window_buckets": window_buckets,
            "horizon_buckets": horizon_buckets,
        })
    return observations


def bucket_stats(rows):
    returns = [row["absolute_forward_return_bps"] for row in rows]
    return {
        "observations": len(rows),
        "mean_absolute_forward_return_bps": statistics.mean(returns) if returns else None,
        "median_absolute_forward_return_bps": statistics.median(returns) if returns else None,
        "mean_forward_return_pct": statistics.mean(row["forward_return_pct"] for row in rows)
        if rows else None,
    }


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    high = [row for row in observations if row["state"] == "high_vpin"]
    normal = [row for row in observations if row["state"] == "normal_vpin"]
    high_stats = bucket_stats(high)
    normal_stats = bucket_stats(normal)
    high_mean = high_stats["mean_absolute_forward_return_bps"]
    normal_mean = normal_stats["mean_absolute_forward_return_bps"]
    edge = high_mean - normal_mean if high_mean is not None and normal_mean is not None else None
    adjusted = edge - paper_cost_bps if edge is not None else None
    candidate = (len(high) >= min_observations and edge is not None
                 and adjusted >= min_edge_bps)
    return {
        "high_vpin": high_stats,
        "normal_vpin_control": normal_stats,
        "high_minus_normal_absolute_edge_bps": edge,
        "paper_cost_bps": paper_cost_bps,
        "cost_adjusted_edge_bps": adjusted,
        "verdict": "vpin_stress_response_candidate" if candidate else "observe_only",
        "evidence": [
            "volume_buckets_and_forward_window_available" if observations
            else "no_complete_volume_buckets_with_forward_window",
            "high_vpin_response_clears_control_hurdle" if candidate
            else "response_below_hurdle_or_insufficient_control",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--days", type=float, default=2.0)
    parser.add_argument("--trade-pages", type=int, default=12)
    parser.add_argument("--bucket-notional", type=float, default=1_000_000.0)
    parser.add_argument("--window-buckets", type=int, default=20)
    parser.add_argument("--horizon-buckets", type=int, default=3)
    parser.add_argument("--high-vpin", type=float, default=0.60)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 1 <= args.trade_pages <= 48 or args.bucket_notional <= 0
            or args.window_buckets <= 0 or args.horizon_buckets <= 0
            or not 0 <= args.high_vpin <= 1 or args.paper_cost_bps < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, bucket, VPIN, cost or observation arguments")

    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/trades", {
        "exchange": args.exchange, "symbol": args.symbol,
        "start_ms": start_ms, "end_ms": end_ms, "limit": 1000, "pages": args.trade_pages,
    }, args.timeout)
    trades = trade_rows(payload)
    buckets = volume_buckets(trades, args.bucket_notional)
    observations = vpin_observations(
        buckets, args.window_buckets, args.horizon_buckets, args.high_vpin,
    )
    summary = summarize(observations, args.min_observations,
                        args.paper_cost_bps, args.min_edge_bps)
    evidence = list(summary.pop("evidence"))
    detail = payload.get("coverage_detail")
    if isinstance(detail, dict) and detail.get("status"):
        evidence.append(f"trades_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_vpin_response_replay",
        "market": {"exchange": args.exchange, "symbol": args.symbol},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "trade_pages": args.trade_pages, "bucket_notional": args.bucket_notional,
            "window_buckets": args.window_buckets, "horizon_buckets": args.horizon_buckets,
            "high_vpin": args.high_vpin, "paper_cost_bps": args.paper_cost_bps,
            "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations,
        },
        "source_counts": {"trades": len(trades), "complete_volume_buckets": len(buckets),
                          "forward_observations": len(observations)},
        "observations": observations,
        "summary": summary,
        "coverage": payload.get("coverage_detail"),
        "evidence": evidence,
        "upstream_errors": ([{"source": "trades", "error": payload["error"]}]
                            if payload.get("error") else []),
        "limitations": [
            "VPIN is a proxy using reported taker side and fixed quote-notional buckets",
            "the final partial bucket is discarded and provider retention bounds the sample",
            "high-VPIN response is non-directional and does not identify informed traders",
            "paper cost is a sensitivity hurdle, not fees, queue, latency, fill or execution modeling",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
