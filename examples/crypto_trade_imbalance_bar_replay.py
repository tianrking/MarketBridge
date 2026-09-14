#!/usr/bin/env python3
"""Replay event-driven trade-imbalance bars from MarketBridge trades.

The falsifiable question is narrow: after a completed event bar reaches a
fixed quote-notional threshold and its signed taker imbalance is large, is the
next fixed number of event bars more likely to continue in the same direction
than an ordinary (balanced) event bar?  This is an event-time research replay,
not a fill, market-making or execution model.
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


def build_imbalance_bars(trades, threshold_notional, max_trades_per_bar):
    """Close a bar at a fixed notional imbalance or a hard trade-count cap."""
    bars = []
    start_ts = None
    signed_notional = 0.0
    total_notional = 0.0
    trade_count = 0
    last_price = None
    last_ts = None
    for trade in trades:
        if start_ts is None:
            start_ts = trade["ts_ms"]
        sign = 1.0 if trade["side"] == "buy" else -1.0
        signed_notional += sign * trade["notional"]
        total_notional += trade["notional"]
        trade_count += 1
        last_price = trade["price"]
        last_ts = trade["ts_ms"]
        threshold_hit = abs(signed_notional) >= threshold_notional
        count_cap_hit = trade_count >= max_trades_per_bar
        if not threshold_hit and not count_cap_hit:
            continue
        ratio = signed_notional / total_notional if total_notional else None
        direction_sign = 1 if signed_notional > 0 else -1 if signed_notional < 0 else 0
        bars.append({
            "start_ts_ms": start_ts,
            "end_ts_ms": last_ts,
            "close_price": last_price,
            "signed_notional": signed_notional,
            "total_notional": total_notional,
            "trade_count": trade_count,
            "imbalance_ratio": ratio,
            "direction_sign": direction_sign,
            "close_reason": "imbalance_threshold" if threshold_hit else "trade_count_cap",
        })
        start_ts = None
        signed_notional = 0.0
        total_notional = 0.0
        trade_count = 0
        last_price = None
        last_ts = None
    return bars


def event_observations(bars, horizon_bars, min_imbalance_ratio):
    observations = []
    for index, bar in enumerate(bars[:-horizon_bars] if horizon_bars > 0 else []):
        future = bars[index + horizon_bars]
        current_price = bar["close_price"]
        future_price = future["close_price"]
        ratio = bar["imbalance_ratio"]
        if current_price is None or future_price is None or current_price <= 0 or future_price <= 0:
            continue
        if ratio is None or abs(ratio) >= min_imbalance_ratio:
            state = "strong_buy" if ratio is not None and ratio >= min_imbalance_ratio else (
                "strong_sell" if ratio is not None and ratio <= -min_imbalance_ratio else "balanced"
            )
        else:
            state = "balanced"
        forward_return_pct = (future_price / current_price - 1.0) * 100.0
        direction_sign = bar["direction_sign"] if state != "balanced" else 0
        aligned_return_pct = direction_sign * forward_return_pct if direction_sign else None
        observations.append({
            "bar_index": index,
            "ts_ms": bar["end_ts_ms"],
            "forward_ts_ms": future["end_ts_ms"],
            "state": state,
            "imbalance_ratio": ratio,
            "signed_notional": bar["signed_notional"],
            "total_notional": bar["total_notional"],
            "trade_count": bar["trade_count"],
            "close_reason": bar["close_reason"],
            "forward_return_pct": forward_return_pct,
            "aligned_return_pct": aligned_return_pct,
            "aligned_return_bps": aligned_return_pct * 100.0 if aligned_return_pct is not None else None,
            "aligned": aligned_return_pct > 0 if aligned_return_pct is not None else None,
        })
    return observations


def state_stats(rows, directional=False):
    returns = [row["forward_return_pct"] for row in rows]
    absolute_bps = [abs(value) * 100.0 for value in returns]
    aligned = [row["aligned_return_bps"] for row in rows if row["aligned_return_bps"] is not None]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(returns) if returns else None,
        "mean_absolute_forward_return_bps": statistics.mean(absolute_bps) if absolute_bps else None,
        "mean_aligned_return_bps": statistics.mean(aligned) if aligned else None,
        "aligned_hit_rate": (sum(value > 0 for value in aligned) / len(aligned)) if aligned else None,
        "directional": directional,
    }


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    directional = [row for row in observations if row["state"] in {"strong_buy", "strong_sell"}]
    balanced = [row for row in observations if row["state"] == "balanced"]
    directional_stats = state_stats(directional, directional=True)
    balanced_stats = state_stats(balanced)
    directional_mean = directional_stats["mean_aligned_return_bps"]
    balanced_mean = balanced_stats["mean_absolute_forward_return_bps"]
    edge = (directional_mean - balanced_mean
            if directional_mean is not None and balanced_mean is not None else None)
    adjusted = edge - paper_cost_bps if edge is not None else None
    candidate = (len(directional) >= min_observations and edge is not None
                 and adjusted >= min_edge_bps)
    return {
        "strong_imbalance": directional_stats,
        "balanced_control": balanced_stats,
        "aligned_minus_balanced_absolute_edge_bps": edge,
        "paper_cost_bps": paper_cost_bps,
        "cost_adjusted_edge_bps": adjusted,
        "verdict": "trade_imbalance_bar_response_candidate" if candidate else "observe_only",
        "evidence": [
            "event_bars_and_forward_event_window_available" if observations
            else "no_complete_event_bars_with_forward_window",
            "directional_continuation_clears_control_hurdle" if candidate
            else "continuation_below_hurdle_or_insufficient_control",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--days", type=float, default=2.0)
    parser.add_argument("--trade-pages", type=int, default=12)
    parser.add_argument("--bar-notional", type=float, default=1_000_000.0)
    parser.add_argument("--max-trades-per-bar", type=int, default=500)
    parser.add_argument("--min-imbalance-ratio", type=float, default=0.60)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 1 <= args.trade_pages <= 48 or args.bar_notional <= 0
            or args.max_trades_per_bar <= 0 or not 0 < args.min_imbalance_ratio <= 1
            or args.horizon_bars <= 0 or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, bar, imbalance, cost or observation arguments")

    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/trades", {
        "exchange": args.exchange, "symbol": args.symbol,
        "start_ms": start_ms, "end_ms": end_ms, "limit": 1000, "pages": args.trade_pages,
    }, args.timeout)
    trades = trade_rows(payload)
    bars = build_imbalance_bars(trades, args.bar_notional, args.max_trades_per_bar)
    observations = event_observations(bars, args.horizon_bars, args.min_imbalance_ratio)
    summary = summarize(observations, args.min_observations,
                        args.paper_cost_bps, args.min_edge_bps)
    evidence = list(summary.pop("evidence"))
    detail = payload.get("coverage_detail")
    if isinstance(detail, dict) and detail.get("status"):
        evidence.append(f"trades_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_trade_imbalance_bar_replay",
        "market": {"exchange": args.exchange, "symbol": args.symbol},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "trade_pages": args.trade_pages, "bar_notional": args.bar_notional,
            "max_trades_per_bar": args.max_trades_per_bar,
            "min_imbalance_ratio": args.min_imbalance_ratio,
            "horizon_bars": args.horizon_bars, "paper_cost_bps": args.paper_cost_bps,
            "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations,
        },
        "source_counts": {"trades": len(trades), "complete_event_bars": len(bars),
                          "forward_observations": len(observations)},
        "observations": observations,
        "summary": summary,
        "coverage": payload.get("coverage_detail"),
        "evidence": evidence,
        "upstream_errors": ([{"source": "trades", "error": payload["error"]}]
                            if payload.get("error") else []),
        "limitations": [
            "event bars use a caller-fixed quote-notional threshold and a hard trade-count cap",
            "aggressor-side labels are provider semantics and the sample is single-venue",
            "event-time horizons are not fixed elapsed minutes and incomplete final bars are discarded",
            "balanced absolute movement is a descriptive control, not a risk-matched benchmark",
            "paper cost is a sensitivity hurdle, not fees, queue, latency, fill or execution modeling",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
