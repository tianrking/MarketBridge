#!/usr/bin/env python3
"""Replay a quarter-hour opening order-flow hypothesis without execution.

The falsifiable question is narrow: after a UTC quarter-hour opens, does the
signed taker-flow imbalance observed during a short opening window align with
the next fixed-horizon perp return?  This is a bounded single-venue replay,
not a timing instruction, order router or performance claim.
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


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            rows.append((ts_ms, close))
    return sorted(dict(rows).items())


def trade_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        notional = number(row.get("notional"))
        side = str(row.get("side", "")).lower()
        if isinstance(ts_ms, int) and notional is not None and notional > 0 and side in {"buy", "sell"}:
            rows.append({"ts_ms": ts_ms, "notional": notional, "side": side})
    return sorted(rows, key=lambda row: row["ts_ms"])


def flow_window(trades, start_ms, end_ms):
    selected = [row for row in trades if start_ms <= row["ts_ms"] < end_ms]
    buy = sum(row["notional"] for row in selected if row["side"] == "buy")
    sell = sum(row["notional"] for row in selected if row["side"] == "sell")
    total = buy + sell
    return {
        "buy_notional": buy if buy else None,
        "sell_notional": sell if sell else None,
        "total_notional": total if total else None,
        "delta_notional": buy - sell if total else None,
        "trade_count": len(selected),
        "ratio": (buy - sell) / total if total else None,
    }


def quarter_hour_observations(candles, trades, window_minutes, horizon_bars,
                              min_flow_ratio, min_notional):
    by_ts = dict(candles)
    timestamps = [ts for ts, _ in candles]
    observations = []
    window_ms = window_minutes * 60_000
    for ts_ms in timestamps:
        if ts_ms % 900_000 != 0:
            continue
        forward_ts = ts_ms + horizon_bars * 60_000
        if forward_ts not in by_ts:
            continue
        flow = flow_window(trades, ts_ms, ts_ms + window_ms)
        ratio = flow["ratio"]
        if ratio is None or abs(ratio) < min_flow_ratio:
            continue
        if flow["total_notional"] is None or flow["total_notional"] < min_notional:
            continue
        close = by_ts[ts_ms]
        forward_close = by_ts[forward_ts]
        forward_return_pct = (forward_close / close - 1.0) * 100.0
        direction = 1 if ratio > 0 else -1
        aligned_return_pct = direction * forward_return_pct
        observations.append({
            "ts_ms": ts_ms,
            "forward_ts_ms": forward_ts,
            "flow": flow,
            "flow_direction": "buy" if direction > 0 else "sell",
            "forward_return_pct": forward_return_pct,
            "aligned_return_pct": aligned_return_pct,
            "aligned_return_bps": aligned_return_pct * 100.0,
            "aligned": aligned_return_pct > 0,
        })
    return observations


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    gross = [row["aligned_return_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {
        "signals": len(observations),
        "aligned_signals": sum(row["aligned"] for row in observations),
        "aligned_hit_rate": (sum(row["aligned"] for row in observations) / len(observations)
                              if observations else None),
        "mean_aligned_return_bps": statistics.mean(gross) if gross else None,
        "median_aligned_return_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "min_edge_bps": min_edge_bps,
        "verdict": "quarter_hour_flow_candidate" if candidate else "observe_only",
        "evidence": [
            "quarter_hour_flow_and_forward_return_available" if observations
            else "no_qualifying_quarter_hour_flow_windows",
            "cost_adjusted_mean_clears_hurdle" if candidate
            else "insufficient_observations_or_mean_below_hurdle",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--trades-exchange", default=None)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--candle-limit", type=int, default=1500)
    parser.add_argument("--trade-pages", type=int, default=12)
    parser.add_argument("--window-minutes", type=int, default=5)
    parser.add_argument("--horizon-bars", type=int, default=240)
    parser.add_argument("--min-flow-ratio", type=float, default=0.20)
    parser.add_argument("--min-notional", type=float, default=0.0)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.candle_limit <= 0 or not 1 <= args.trade_pages <= 48
            or not 1 <= args.window_minutes <= 15 or args.horizon_bars <= 0
            or not 0 < args.min_flow_ratio < 1 or args.min_notional < 0
            or args.paper_cost_bps < 0 or args.min_edge_bps < 0
            or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid window, flow, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    trades_exchange = args.trades_exchange or args.exchange
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": "1m", "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.candle_limit, 1500),
    }, args.timeout)
    trade_payload = fetch(args.base_url, "/v1/history/trades", {
        "exchange": trades_exchange, "symbol": args.symbol,
        "start_ms": start_ms, "end_ms": end_ms, "limit": 1000, "pages": args.trade_pages,
    }, args.timeout)
    candles = candle_rows(candle_payload)
    trades = trade_rows(trade_payload)
    observations = quarter_hour_observations(
        candles, trades, args.window_minutes, args.horizon_bars,
        args.min_flow_ratio, args.min_notional,
    )
    summary = summarize(observations, args.min_observations, args.paper_cost_bps, args.min_edge_bps)
    evidence = list(summary.pop("evidence"))
    for name, payload in (("candles", candle_payload), ("trades", trade_payload)):
        detail = payload.get("coverage_detail")
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{name}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_quarter_hour_flow_replay",
        "market": {"exchange": args.exchange, "trades_exchange": trades_exchange,
                   "symbol": args.symbol, "market": args.market, "interval": "1m"},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"window_minutes": args.window_minutes, "horizon_bars": args.horizon_bars,
                    "min_flow_ratio": args.min_flow_ratio, "min_notional": args.min_notional,
                    "trade_pages": args.trade_pages, "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
        "source_counts": {"candles": len(candles), "trades": len(trades)},
        "observations": observations,
        "summary": summary,
        "coverage": {"candles": candle_payload.get("coverage_detail"),
                      "trades": trade_payload.get("coverage_detail")},
        "evidence": evidence,
        "provenance": {
            "research": "https://arxiv.org/abs/2607.09426",
            "mapping": "quarter-hour opening signed taker-flow response subset",
            "source_data": "MarketBridge candles and bounded public trade history",
        },
        "upstream_errors": [
            {"source": name, "error": payload.get("error")}
            for name, payload in (("candles", candle_payload), ("trades", trade_payload))
            if payload.get("error")
        ],
        "limitations": [
            "quarter-hour phase is UTC and does not establish a causal session effect",
            "public trade history is bounded and may not cover every quarter-hour window",
            "taker-side labels are provider semantics; flow is single-venue and not global",
            "paper cost is a sensitivity input, not fees, queue, latency or fill modeling",
            "no order, wallet, allocation or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
