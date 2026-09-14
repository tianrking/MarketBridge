#!/usr/bin/env python3
"""Replay provider long/short account ratio plus OI-change states.

The case tests whether account-ratio imbalance with rising OI differs from
imbalance with falling OI. Provider semantics remain explicit: Bybit reports
holder counts, while Binance can report global account shares, top-trader
account shares, or top-trader position shares.
Neither is notional exposure or ownership.
"""

import argparse
import json
import statistics
import time

from crypto_taker_oi_response_replay import (
    candle_points,
    fetch,
    forward_return,
    oi_change_at,
    oi_points,
    number,
)


def ratio_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp, imbalance = row.get("ts_ms"), number(row.get("imbalance"))
        if isinstance(timestamp, int) and imbalance is not None and -1.0 <= imbalance <= 1.0:
            points.append((timestamp, imbalance))
    return sorted(set(points))


def classify_state(imbalance, oi_change_pct, ratio_threshold, oi_threshold):
    if imbalance is None or oi_change_pct is None:
        return "observe_only_missing_alignment"
    if imbalance >= ratio_threshold and oi_change_pct >= oi_threshold:
        return "long_holder_crowding_oi_rising"
    if imbalance <= -ratio_threshold and oi_change_pct >= oi_threshold:
        return "short_holder_crowding_oi_rising"
    if imbalance >= ratio_threshold and oi_change_pct <= -oi_threshold:
        return "long_holder_unwinding_oi_falling"
    if imbalance <= -ratio_threshold and oi_change_pct <= -oi_threshold:
        return "short_holder_unwinding_oi_falling"
    return "ordinary_account_ratio_state"


def summarize(rows):
    result = {}
    for state in sorted({row["state"] for row in rows}):
        selected = [row for row in rows if row["state"] == state]
        returns = [row["forward_return_pct"] for row in selected if row["forward_return_pct"] is not None]
        result[state] = {
            "observations": len(selected),
            "forward_observations": len(returns),
            "mean_forward_return_pct": statistics.mean(returns) if returns else None,
            "mean_absolute_forward_return_pct": statistics.mean(abs(value) for value in returns) if returns else None,
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="bybit")
    parser.add_argument("--ratio-scope", default="top_trader",
                        choices=("top_trader", "top_trader_position", "global"),
                        help="Binance account-ratio scope; ignored by Bybit")
    parser.add_argument("--period", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--limit", type=int, default=50)
    parser.add_argument("--ratio-threshold", type=float, default=0.10)
    parser.add_argument("--oi-threshold", type=float, default=0.10)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit <= 0 or args.ratio_threshold < 0
            or args.oi_threshold < 0 or args.horizon_bars <= 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, threshold, horizon, observation or timeout argument")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "exchange": args.exchange, "start_ms": start_ms,
              "end_ms": now_ms, "limit": min(args.limit, 500)}
    ratio_payload = fetch(args.base_url, "/v1/history/account-ratio",
                          {**common, "period": args.period,
                           "scope": args.ratio_scope if args.exchange.lower() == "binance" else None}, args.timeout)
    oi_payload = fetch(args.base_url, "/v1/history/open-interest",
                       {**common, "interval": args.period}, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles",
                          {**common, "candle_type": "perp", "interval": args.period}, args.timeout)
    ratios, oi, prices = ratio_points(ratio_payload), oi_points(oi_payload), candle_points(price_payload)
    rows = []
    for timestamp, imbalance in ratios:
        oi_change = oi_change_at(timestamp, oi)
        rows.append({"ts_ms": timestamp, "account_ratio_imbalance": imbalance,
                     "oi_change_pct": oi_change,
                     "state": classify_state(imbalance, oi_change, args.ratio_threshold, args.oi_threshold),
                     "forward_return_pct": forward_return(timestamp, prices, args.horizon_bars)})
    qualifying = [row for row in rows if row["state"] != "observe_only_missing_alignment"
                  and row["forward_return_pct"] is not None]
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in (("account_ratio", ratio_payload), ("open_interest", oi_payload), ("price", price_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_account_ratio_oi_response_replay", "symbol": args.symbol,
        "venue": args.exchange, "ratio_scope": args.ratio_scope, "period": args.period,
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"ratio_threshold": args.ratio_threshold, "oi_threshold": args.oi_threshold,
                    "horizon_bars": args.horizon_bars, "min_observations": args.min_observations},
        "source_counts": {"account_ratio": len(ratios), "open_interest": len(oi), "price_bars": len(prices)},
        "observations": rows, "summary": summarize(rows),
        "coverage": {"account_ratio": ratio_payload.get("coverage_detail"),
                      "open_interest": oi_payload.get("coverage_detail"),
                      "price": price_payload.get("coverage_detail")},
        "verdict": "account-ratio response candidate" if len(qualifying) >= args.min_observations else "observe only",
        "upstream_errors": errors,
        "limitations": [
            "ratio semantics are provider-specific: Bybit holder counts, Binance global account shares, top-trader account shares, or top-trader position shares",
            "OI is aggregate and does not identify long/short ownership or trader intent",
            "forward return is descriptive and excludes fees, funding cash flow, slippage and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
