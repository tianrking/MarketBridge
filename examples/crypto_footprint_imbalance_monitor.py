#!/usr/bin/env python3
"""Observe footprint imbalance pressure from MarketBridge's rolling trade store."""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}/v1/market/footprint?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def summarize_footprints(rows, min_delta_ratio, min_stacked_levels):
    if not rows:
        return {"state": "observe_only_missing_footprint", "footprints": [], "research_only": True}
    observations = []
    for row in sorted(rows, key=lambda item: item.get("bucket_start_ms", 0)):
        total = abs(number(row.get("ask_notional")) or 0.0) + abs(number(row.get("bid_notional")) or 0.0)
        delta = number(row.get("delta_notional"))
        ratio = delta / total if total > 0 and delta is not None else None
        bid_stack = row.get("stacked_imbalances_bid") or []
        ask_stack = row.get("stacked_imbalances_ask") or []
        side = "buy" if ratio is not None and ratio >= min_delta_ratio else (
            "sell" if ratio is not None and ratio <= -min_delta_ratio else "balanced")
        if len(bid_stack) >= min_stacked_levels and len(bid_stack) > len(ask_stack):
            side = "stacked_bid_pressure"
        elif len(ask_stack) >= min_stacked_levels and len(ask_stack) > len(bid_stack):
            side = "stacked_ask_pressure"
        observations.append({"bucket_start_ms": row.get("bucket_start_ms"),
                             "bucket_end_ms": row.get("bucket_end_ms"),
                             "delta_notional": delta, "delta_ratio": ratio,
                             "total_trades": row.get("total_trades"),
                             "stacked_bid_levels": len(bid_stack),
                             "stacked_ask_levels": len(ask_stack), "pressure": side})
    latest = observations[-1]
    state = "footprint_bid_pressure" if latest["pressure"] in {"buy", "stacked_bid_pressure"} else (
        "footprint_ask_pressure" if latest["pressure"] in {"sell", "stacked_ask_pressure"}
        else "footprint_balanced")
    return {"state": state, "latest": latest, "footprints": observations,
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--market", default="perp", choices=("spot", "perp"))
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--interval-ms", type=int, default=60_000)
    parser.add_argument("--scale", type=float, default=1.0)
    parser.add_argument("--imbalance-ratio", type=float, default=3.0)
    parser.add_argument("--imbalance-volume", type=float, default=0.0)
    parser.add_argument("--stacked-imbalance-range", type=int, default=3)
    parser.add_argument("--limit", type=int, default=20)
    parser.add_argument("--min-delta-ratio", type=float, default=0.20)
    parser.add_argument("--min-stacked-levels", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.interval_ms <= 0 or args.scale <= 0 or args.imbalance_ratio <= 0
            or args.imbalance_volume < 0 or args.stacked_imbalance_range <= 0 or args.limit <= 0
            or not 0 < args.min_delta_ratio < 1 or args.min_stacked_levels <= 0 or args.timeout <= 0):
        parser.error("invalid footprint interval, scale, threshold or timeout")
    payload = fetch(args.base_url, {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval_ms": args.interval_ms, "scale": args.scale,
        "imbalance_ratio": args.imbalance_ratio, "imbalance_volume": args.imbalance_volume,
        "stacked_imbalance_range": args.stacked_imbalance_range, "include_trades": "false",
        "limit": args.limit,
    }, args.timeout)
    summary = summarize_footprints(payload.get("footprints", []), args.min_delta_ratio, args.min_stacked_levels)
    print(json.dumps({"strategy": "crypto_footprint_imbalance_monitor",
                      "market": {"exchange": args.exchange, "market": args.market, "symbol": args.symbol},
                      "filters": {"interval_ms": args.interval_ms, "scale": args.scale,
                                  "imbalance_ratio": args.imbalance_ratio,
                                  "imbalance_volume": args.imbalance_volume,
                                  "stacked_imbalance_range": args.stacked_imbalance_range,
                                  "min_delta_ratio": args.min_delta_ratio,
                                  "min_stacked_levels": args.min_stacked_levels},
                      "summary": summary, "upstream_errors": payload.get("errors", []),
                      "limitations": [
                          "footprint uses the rolling in-memory trade buffer and is not historical replay",
                          "stacked imbalance is a price-bin statistic, not resting order-book liquidity",
                          "taker-side semantics and retention are provider/runtime dependent",
                          "no price forecast, allocation, order, wallet or execution path is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
