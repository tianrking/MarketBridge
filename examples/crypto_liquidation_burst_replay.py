#!/usr/bin/env python3
"""Replay whether large rolling liquidation bursts precede larger moves.

The falsifiable hypothesis is intentionally non-directional: when public
liquidation notional in a trailing window exceeds a threshold, the next fixed
price window may have larger absolute movement than ordinary candle windows.
It does not infer long/short liquidation semantics, predict direction or
simulate a trade.
"""

import argparse
import json
import statistics
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


def liquidation_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        ts_ms = row.get("ts_ms")
        notional = number(row.get("notional"))
        if isinstance(ts_ms, int) and notional is not None and notional > 0:
            rows.append({"ts_ms": ts_ms, "notional": notional, "side": row.get("side")})
    return sorted(rows, key=lambda row: row["ts_ms"])


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def burst_observations(events, candles, window_ms, horizon_bars, threshold,
                       cooldown_bars):
    if window_ms <= 0 or horizon_bars <= 0 or threshold < 0 or cooldown_bars < 0:
        return []
    observations = []
    last_trigger_ts = None
    for index, (ts_ms, close) in enumerate(candles):
        if index + horizon_bars >= len(candles):
            break
        selected = [row for row in events if ts_ms - window_ms < row["ts_ms"] <= ts_ms]
        total = sum(row["notional"] for row in selected)
        if total < threshold:
            continue
        if (last_trigger_ts is not None
                and ts_ms - last_trigger_ts < cooldown_bars * _bar_ms(candles)):
            continue
        future_close = candles[index + horizon_bars][1]
        forward = forward_return_pct(close, future_close)
        if forward is None:
            continue
        sell = sum(row["notional"] for row in selected
                   if str(row.get("side", "")).lower() == "sell")
        buy = sum(row["notional"] for row in selected
                  if str(row.get("side", "")).lower() == "buy")
        observations.append({
            "ts_ms": ts_ms,
            "window_notional": total,
            "sell_notional": sell if sell > 0 else None,
            "buy_notional": buy if buy > 0 else None,
            "event_count": len(selected),
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
            "horizon_ts_ms": candles[index + horizon_bars][0],
        })
        last_trigger_ts = ts_ms
    return observations


def _bar_ms(candles):
    if len(candles) < 2:
        return 1
    return max(1, candles[1][0] - candles[0][0])


def summarize(observations, candles, horizon_bars, min_observations, min_abs_edge_bps):
    all_moves = [
        abs(forward_return_pct(candles[index][1], candles[index + horizon_bars][1]))
        for index in range(max(0, len(candles) - horizon_bars))
        if forward_return_pct(candles[index][1], candles[index + horizon_bars][1]) is not None
    ]
    burst_moves = [row["forward_abs_return_pct"] for row in observations]
    burst_mean = statistics.mean(burst_moves) if burst_moves else None
    baseline_mean = statistics.mean(all_moves) if all_moves else None
    edge_bps = ((burst_mean - baseline_mean) * 100.0
                if burst_mean is not None and baseline_mean is not None else None)
    candidate = (len(observations) >= min_observations and edge_bps is not None
                 and edge_bps >= min_abs_edge_bps)
    return {
        "burst_observations": len(observations),
        "baseline_forward_windows": len(all_moves),
        "mean_burst_abs_return_pct": burst_mean,
        "mean_baseline_abs_return_pct": baseline_mean,
        "absolute_move_edge_bps": edge_bps,
        "positive_forward_return_fraction": (
            sum(row["forward_return_pct"] > 0 for row in observations) / len(observations)
            if observations else None
        ),
        "median_window_notional": (
            statistics.median(row["window_notional"] for row in observations)
            if observations else None
        ),
        "verdict": "liquidation_burst_move_candidate" if candidate else "observe_only",
        "evidence": [
            "rolling_liquidation_window_available" if observations else "no_threshold_burst_observed",
            "forward_price_windows_available" if all_moves else "missing_forward_price_windows",
            "burst_absolute_move_above_baseline" if candidate
            else "burst_edge_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", choices=("okx", "coinex"), default="okx")
    parser.add_argument("--price-exchange", choices=("okx", "binance"), default=None)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--liquidation-limit", type=int, default=100)
    parser.add_argument("--candle-limit", type=int, default=320)
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--threshold-notional", type=float, default=1_000_000_000.0)
    parser.add_argument("--cooldown-bars", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.liquidation_limit <= 0 or args.candle_limit <= 0 or args.window_hours <= 0
            or args.horizon_bars <= 0 or args.threshold_notional < 0
            or args.cooldown_bars < 0 or args.min_observations <= 0
            or args.min_abs_edge_bps < 0):
        parser.error("invalid limits, window, threshold, horizon or observation arguments")
    price_exchange = args.price_exchange or args.exchange
    liquidation_payload = fetch(args.base_url, "/v1/history/liquidations", {
        "exchange": args.exchange, "symbol": args.symbol,
        "limit": min(args.liquidation_limit, 100),
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": price_exchange, "symbol": args.symbol,
        "candle_type": "perp", "interval": args.interval,
        "limit": min(args.candle_limit, 1000),
    }, args.timeout)
    events = liquidation_rows(liquidation_payload)
    candles = candle_rows(candle_payload)
    observations = burst_observations(
        events, candles, int(args.window_hours * 3_600_000), args.horizon_bars,
        args.threshold_notional, args.cooldown_bars,
    )
    coverage = liquidation_payload.get("coverage_detail")
    summary = summarize(observations, candles, args.horizon_bars,
                        args.min_observations, args.min_abs_edge_bps)
    evidence = list(summary.pop("evidence"))
    if coverage:
        evidence.append(f"liquidation_coverage_{coverage.get('status', 'reported')}")
    print(json.dumps({
        "strategy": "crypto_liquidation_burst_replay",
        "exchange": args.exchange,
        "price_exchange": price_exchange,
        "symbol": args.symbol,
        "filters": {
            "window_hours": args.window_hours,
            "horizon_bars": args.horizon_bars,
            "threshold_notional": args.threshold_notional,
            "cooldown_bars": args.cooldown_bars,
            "min_observations": args.min_observations,
            "min_abs_edge_bps": args.min_abs_edge_bps,
        },
        "events_scanned": len(events),
        "candle_points": len(candles),
        "liquidation_coverage": coverage,
        "observations": observations,
        "summary": summary,
        "evidence": evidence,
        "limitations": [
            "side labels are retained as metadata and are not interpreted as long/short liquidation truth",
            "provider history is bounded and may not cover the requested rolling window",
            "no fees, fills, slippage, latency, position sizing or directional trade model",
        ],
        "upstream_errors": [value for value in (
            liquidation_payload.get("error"), candle_payload.get("error")
        ) if value],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
