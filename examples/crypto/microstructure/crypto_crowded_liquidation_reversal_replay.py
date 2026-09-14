#!/usr/bin/env python3
"""Replay a crowded-positioning plus liquidation-flush hypothesis.

The research lead is intentionally narrowed to observable fields: an extreme
provider account-ratio imbalance, a point-in-time OI decrease, and a burst of
observed liquidation notional. The replay compares later signed and absolute
BTC responses with ordinary aligned observations. Provider side labels are
retained as metadata; they are not treated as universal long/short truth.
"""

import argparse
import bisect
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
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def ratio_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp, imbalance = row.get("ts_ms"), number(row.get("imbalance"))
        if isinstance(timestamp, int) and imbalance is not None and -1.0 <= imbalance <= 1.0:
            points.append((timestamp, imbalance))
    return sorted(set(points))


def oi_points(payload):
    points = []
    for row in payload.get("rows", []):
        timestamp, value = row.get("ts_ms"), number(row.get("open_interest"))
        if isinstance(timestamp, int) and value is not None and value > 0:
            points.append((timestamp, value))
    return sorted(set(points))


def candle_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp, close = row.get("open_time_ms"), number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def liquidation_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        timestamp, notional = row.get("ts_ms"), number(row.get("notional"))
        if isinstance(timestamp, int) and notional is not None and notional > 0:
            rows.append({
                "ts_ms": timestamp,
                "notional": notional,
                "side": str(row.get("side", "")).lower(),
            })
    return sorted(rows, key=lambda row: row["ts_ms"])


def oi_change_at(timestamp, points):
    prior = [point for point in points if point[0] <= timestamp]
    if len(prior) < 2 or prior[-2][1] <= 0:
        return None
    return (prior[-1][1] / prior[-2][1] - 1.0) * 100.0


def forward_return(timestamp, points, horizon_bars):
    timestamps = [point[0] for point in points]
    index = bisect.bisect_left(timestamps, timestamp)
    future_index = index + horizon_bars
    if horizon_bars <= 0 or index >= len(points) or future_index >= len(points):
        return None
    baseline, future = points[index][1], points[future_index][1]
    if baseline <= 0 or future <= 0:
        return None
    return (future / baseline - 1.0) * 100.0


def liquidation_window(events, timestamp, window_ms):
    selected = [event for event in events
                if timestamp - window_ms < event["ts_ms"] <= timestamp]
    total = sum(event["notional"] for event in selected)
    sell = sum(event["notional"] for event in selected if event["side"] == "sell")
    buy = sum(event["notional"] for event in selected if event["side"] == "buy")
    return {
        "total_notional": total,
        "sell_notional": sell if sell > 0 else None,
        "buy_notional": buy if buy > 0 else None,
        "event_count": len(selected),
    }


def classify_state(imbalance, oi_change_pct, liquidation, ratio_threshold,
                   oi_drop_threshold, min_notional):
    if imbalance is None or oi_change_pct is None or liquidation is None:
        return "observe_only_missing_alignment"
    if oi_change_pct > -oi_drop_threshold:
        return "ordinary_positioning_state"
    if (imbalance >= ratio_threshold
            and (liquidation["sell_notional"] or 0.0) >= min_notional):
        return "long_crowding_flush_context"
    if (imbalance <= -ratio_threshold
            and (liquidation["buy_notional"] or 0.0) >= min_notional):
        return "short_crowding_flush_context"
    return "oi_drop_without_directional_flush"


def build_observations(ratios, oi, events, prices, liquidation_window_ms,
                       ratio_threshold, oi_drop_threshold, min_notional,
                       horizon_bars):
    rows = []
    for timestamp, imbalance in ratios:
        metrics = liquidation_window(events, timestamp, liquidation_window_ms)
        oi_change = oi_change_at(timestamp, oi)
        forward = forward_return(timestamp, prices, horizon_bars)
        state = classify_state(
            imbalance, oi_change, metrics, ratio_threshold,
            oi_drop_threshold, min_notional,
        )
        rows.append({
            "ts_ms": timestamp,
            "account_ratio_imbalance": imbalance,
            "oi_change_pct": oi_change,
            "liquidation_window": metrics,
            "state": state,
            "forward_return_pct": forward,
            "absolute_forward_return_pct": abs(forward) if forward is not None else None,
        })
    return rows


def summarize(rows, min_observations, min_abs_edge_bps):
    states = (
        "long_crowding_flush_context",
        "short_crowding_flush_context",
        "ordinary_positioning_state",
        "oi_drop_without_directional_flush",
        "observe_only_missing_alignment",
    )
    by_state = {}
    for state in states:
        selected = [row for row in rows
                    if row["state"] == state and row["forward_return_pct"] is not None]
        signed = [row["forward_return_pct"] for row in selected]
        absolute = [abs(value) for value in signed]
        by_state[state] = {
            "observations": len(selected),
            "mean_forward_return_pct": statistics.mean(signed) if signed else None,
            "mean_absolute_forward_return_pct": statistics.mean(absolute) if absolute else None,
            "positive_fraction": sum(value > 0 for value in signed) / len(signed)
            if signed else None,
        }
    flush = [by_state[state]["mean_absolute_forward_return_pct"]
             for state in ("long_crowding_flush_context", "short_crowding_flush_context")
             if by_state[state]["mean_absolute_forward_return_pct"] is not None]
    ordinary = by_state["ordinary_positioning_state"]["mean_absolute_forward_return_pct"]
    edge_bps = (statistics.mean(flush) - ordinary) * 100.0 \
        if flush and ordinary is not None else None
    flush_observations = sum(by_state[state]["observations"]
                             for state in ("long_crowding_flush_context",
                                           "short_crowding_flush_context"))
    candidate = (flush_observations >= min_observations
                 and by_state["ordinary_positioning_state"]["observations"] >= min_observations
                 and edge_bps is not None and edge_bps >= min_abs_edge_bps)
    return {
        "by_state": by_state,
        "flush_observations": flush_observations,
        "flush_minus_ordinary_absolute_response_edge_bps": edge_bps,
        "min_observations": min_observations,
        "min_abs_edge_bps": min_abs_edge_bps,
        "verdict": "crowded_liquidation_response_candidate" if candidate else "observe_only",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--ratio-exchange", default="binance")
    parser.add_argument("--ratio-scope", default="top_trader",
                        choices=("top_trader", "top_trader_position", "global"))
    parser.add_argument("--liquidation-exchange", choices=("okx", "coinex"), default="okx")
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--period", default="1h")
    parser.add_argument("--days", type=float, default=14.0)
    parser.add_argument("--ratio-limit", type=int, default=500)
    parser.add_argument("--ratio-pages", type=int, default=4)
    parser.add_argument("--oi-limit", type=int, default=500)
    parser.add_argument("--oi-pages", type=int, default=4)
    parser.add_argument("--price-limit", type=int, default=1500)
    parser.add_argument("--price-pages", type=int, default=4)
    parser.add_argument("--liquidation-limit", type=int, default=100)
    parser.add_argument("--liquidation-window-hours", type=float, default=24.0)
    parser.add_argument("--ratio-threshold", type=float, default=0.10)
    parser.add_argument("--oi-drop-threshold", type=float, default=0.10)
    parser.add_argument("--min-liquidation-notional", type=float, default=1_000_000.0)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.ratio_limit <= 0 or args.oi_limit <= 0
            or args.price_limit <= 0 or not 0 <= args.ratio_threshold <= 1
            or args.oi_drop_threshold < 0 or args.min_liquidation_notional < 0
            or args.ratio_pages < 1 or args.ratio_pages > 48
            or args.oi_pages < 1 or args.oi_pages > 96
            or args.price_pages < 1 or args.price_pages > 48
            or args.liquidation_window_hours <= 0 or args.liquidation_limit <= 0
            or args.horizon_bars <= 0 or args.min_observations <= 0
            or args.min_abs_edge_bps < 0 or args.timeout <= 0):
        parser.error("invalid window, pages, threshold, horizon or observation argument")
    now_ms = int(time.time() * 1000)
    start_ms = now_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "start_ms": start_ms, "end_ms": now_ms}
    ratio_payload = fetch(args.base_url, "/v1/history/account-ratio", {
        **common, "exchange": args.ratio_exchange, "scope": args.ratio_scope,
        "period": args.period, "limit": min(args.ratio_limit, 500),
        "pages": args.ratio_pages,
    }, args.timeout)
    oi_payload = fetch(args.base_url, "/v1/history/open-interest", {
        **common, "exchange": args.ratio_exchange, "interval": args.period,
        "limit": min(args.oi_limit, 500), "pages": args.oi_pages,
    }, args.timeout)
    liquidation_payload = fetch(args.base_url, "/v1/history/liquidations", {
        **common, "exchange": args.liquidation_exchange,
        "limit": min(args.liquidation_limit, 100),
    }, args.timeout)
    price_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.price_exchange, "candle_type": "perp",
        "interval": args.period, "limit": min(args.price_limit, 1500),
        "pages": args.price_pages,
    }, args.timeout)
    ratios = ratio_points(ratio_payload)
    oi = oi_points(oi_payload)
    events = liquidation_rows(liquidation_payload)
    prices = candle_points(price_payload)
    rows = build_observations(
        ratios, oi, events, prices,
        int(args.liquidation_window_hours * 3_600_000),
        args.ratio_threshold, args.oi_drop_threshold,
        args.min_liquidation_notional, args.horizon_bars,
    )
    payloads = (("account_ratio", ratio_payload), ("open_interest", oi_payload),
                ("liquidations", liquidation_payload), ("price", price_payload))
    errors = [{"source": name, "error": payload["error"]}
              for name, payload in payloads if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_crowded_liquidation_reversal_replay",
        "hypothesis": "extreme account-ratio imbalance plus OI decrease and observed side-specific liquidation notional may separate later BTC response from ordinary positioning",
        "market": {"symbol": args.symbol, "ratio_exchange": args.ratio_exchange,
                   "ratio_scope": args.ratio_scope, "liquidation_exchange": args.liquidation_exchange,
                   "price_exchange": args.price_exchange, "period": args.period, "timezone": "UTC"},
        "window": {"start_ms": start_ms, "end_ms": now_ms, "days": args.days},
        "filters": {"ratio_threshold": args.ratio_threshold,
                    "oi_drop_threshold": args.oi_drop_threshold,
                    "min_liquidation_notional": args.min_liquidation_notional,
                    "liquidation_window_hours": args.liquidation_window_hours,
                    "ratio_pages": args.ratio_pages, "oi_pages": args.oi_pages,
                    "price_pages": args.price_pages, "horizon_bars": args.horizon_bars,
                    "min_observations": args.min_observations,
                    "min_abs_edge_bps": args.min_abs_edge_bps},
        "source_counts": {"account_ratio": len(ratios), "open_interest": len(oi),
                          "liquidation_events": len(events), "price_bars": len(prices),
                          "observations": len(rows)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations, args.min_abs_edge_bps),
        "coverage": {name: payload.get("coverage_detail") for name, payload in payloads},
        "upstream_errors": errors,
        "limitations": [
            "account-ratio semantics are provider-specific and are not notional ownership",
            "OI is aggregate and does not identify trader intent or position side",
            "liquidation side labels are provider metadata and are not universal long/short truth",
            "OKX/CoinEx liquidation retention is bounded and may omit the requested window",
            "forward response excludes fees, funding cash flow, fills, slippage, latency and execution",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
