#!/usr/bin/env python3
"""Replay a price-versus-CVD divergence and forward-reversal hypothesis.

The falsifiable hypothesis is narrow: when price moves materially over a
lookback window while aggressive taker-flow delta points the other way, does
the next fixed candle window move against the prior price move?  This is a
bounded single-venue flow study, not a directional order or execution model.
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
    return sorted(set(rows))


def trade_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        notional = number(row.get("notional"))
        side = str(row.get("side", "")).lower()
        if isinstance(ts_ms, int) and notional is None:
            continue
        if isinstance(ts_ms, int) and notional is not None and notional > 0 and side in {"buy", "sell"}:
            rows.append({"ts_ms": ts_ms, "notional": notional, "side": side})
    return sorted(rows, key=lambda row: row["ts_ms"])


def flow_window(rows, start_ms, end_ms):
    selected = [row for row in rows if start_ms <= row["ts_ms"] <= end_ms]
    buy = sum(row["notional"] for row in selected if row["side"] == "buy")
    sell = sum(row["notional"] for row in selected if row["side"] == "sell")
    total = buy + sell
    return {
        "buy_notional": buy if buy else None,
        "sell_notional": sell if sell else None,
        "delta_notional": buy - sell if total else None,
        "total_notional": total if total else None,
        "trade_count": len(selected),
        "ratio": (buy - sell) / total if total else None,
    }


def divergence_observations(candles, trades, lookback_bars, horizon_bars,
                            min_price_move_pct, min_flow_ratio):
    observations = []
    for index in range(lookback_bars, len(candles) - horizon_bars):
        start_ts, start_close = candles[index - lookback_bars]
        current_ts, current_close = candles[index]
        future_ts, future_close = candles[index + horizon_bars]
        price_move_pct = (current_close / start_close - 1.0) * 100.0
        flow = flow_window(trades, start_ts, current_ts)
        ratio = flow["ratio"]
        if ratio is None or abs(price_move_pct) < min_price_move_pct:
            continue
        direction = 1 if price_move_pct > 0 and ratio <= -min_flow_ratio else -1 if price_move_pct < 0 and ratio >= min_flow_ratio else 0
        if direction == 0:
            continue
        future_return_pct = (future_close / current_close - 1.0) * 100.0
        aligned_reversal_pct = -direction * future_return_pct
        observations.append({
            "ts_ms": current_ts,
            "forward_ts_ms": future_ts,
            "price_move_pct": price_move_pct,
            "flow": flow,
            "divergence": "bearish" if direction > 0 else "bullish",
            "expected_reversal_sign": -direction,
            "future_return_pct": future_return_pct,
            "aligned_reversal_pct": aligned_reversal_pct,
            "reversed": aligned_reversal_pct > 0,
        })
    return observations


def summarize(observations, min_observations, min_edge_bps, paper_cost_bps=0.0):
    gross = [row["aligned_reversal_pct"] * 100.0 for row in observations]
    adjusted = [edge - paper_cost_bps for edge in gross]
    candidate = (len(observations) >= min_observations and adjusted
                 and statistics.mean(adjusted) >= min_edge_bps)
    return {
        "signals": len(observations),
        "reversal_signals": sum(row["reversed"] for row in observations),
        "reversal_hit_rate": (sum(row["reversed"] for row in observations) / len(observations)
                              if observations else None),
        "mean_aligned_reversal_bps": statistics.mean(gross) if gross else None,
        "median_aligned_reversal_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_reversal_bps": statistics.mean(adjusted) if adjusted else None,
        "verdict": "cvd_divergence_reversal_candidate" if candidate else "observe_only",
        "evidence": [
            "price_and_taker_flow_divergence_available" if observations else "no_divergence_signal",
            "cost_adjusted_reversal_above_threshold" if candidate
            else "reversal_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--trades-exchange", default=None)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--candle-limit", type=int, default=500)
    parser.add_argument("--trade-pages", type=int, default=12)
    parser.add_argument("--lookback-bars", type=int, default=12)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-price-move-pct", type=float, default=0.5)
    parser.add_argument("--min-flow-ratio", type=float, default=0.2)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.candle_limit <= 0 or not 1 <= args.trade_pages <= 48
            or args.lookback_bars <= 0 or args.horizon_bars <= 0 or args.min_price_move_pct < 0
            or not 0 < args.min_flow_ratio < 1 or args.paper_cost_bps < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid windows, flow thresholds, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    trades_exchange = args.trades_exchange or args.exchange
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.candle_limit, 1500),
    }, args.timeout)
    trade_payload = fetch(args.base_url, "/v1/history/trades", {
        "exchange": trades_exchange, "symbol": args.symbol,
        "start_ms": start_ms, "end_ms": end_ms, "limit": 1000, "pages": args.trade_pages,
    }, args.timeout)
    candles = candle_rows(candle_payload)
    trades = trade_rows(trade_payload)
    observations = divergence_observations(
        candles, trades, args.lookback_bars, args.horizon_bars,
        args.min_price_move_pct, args.min_flow_ratio,
    )
    summary = summarize(observations, args.min_observations, args.min_edge_bps, args.paper_cost_bps)
    evidence = list(summary.pop("evidence"))
    for name, payload in (("candles", candle_payload), ("trades", trade_payload)):
        detail = payload.get("coverage_detail")
        if isinstance(detail, dict) and detail.get("status"):
            evidence.append(f"{name}_coverage_{detail['status']}")
    print(json.dumps({
        "strategy": "crypto_cvd_divergence_replay",
        "market": {"exchange": args.exchange, "trades_exchange": trades_exchange,
                   "symbol": args.symbol, "market": args.market, "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {
            "lookback_bars": args.lookback_bars, "horizon_bars": args.horizon_bars,
            "min_price_move_pct": args.min_price_move_pct,
            "min_flow_ratio": args.min_flow_ratio, "trade_pages": args.trade_pages,
            "paper_cost_bps": args.paper_cost_bps, "min_edge_bps": args.min_edge_bps,
            "min_observations": args.min_observations,
        },
        "source_counts": {"candles": len(candles), "trades": len(trades)},
        "observations": observations,
        "summary": summary,
        "coverage": {"candles": candle_payload.get("coverage_detail"),
                      "trades": trade_payload.get("coverage_detail")},
        "evidence": evidence,
        "upstream_errors": [
            {"source": name, "error": payload.get("error")}
            for name, payload in (("candles", candle_payload), ("trades", trade_payload))
            if payload.get("error")
        ],
        "limitations": [
            "CVD is single-venue public taker flow, not a complete global order-flow ledger",
            "trade-side classification and provider retention can change coverage",
            "divergence and forward reversal are descriptive associations, not causal forecasts",
            "paper cost is a sensitivity hurdle, not venue fees, queue, latency or fill modeling",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
