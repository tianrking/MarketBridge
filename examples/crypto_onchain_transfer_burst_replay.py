#!/usr/bin/env python3
"""Replay whether large on-chain transfer bursts precede larger moves.

The falsifiable hypothesis is deliberately non-directional: when the public
transfer cache contains unusually large aggregate USD value in a trailing
window, the next fixed candle window may have larger absolute movement than
ordinary windows. Transfer direction and asset labels remain metadata; this is
not a net-flow, price forecast, or wallet strategy.
"""

import argparse
import json
import statistics
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}" + f"{path}?{query}")
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


def transfer_rows(payload):
    rows = []
    for row in payload.get("transfers", []):
        ts_ms = row.get("ts_ms")
        amount_usd = number(row.get("amount_usd"))
        if isinstance(ts_ms, int) and amount_usd is not None and amount_usd > 0:
            rows.append({
                "ts_ms": ts_ms,
                "amount_usd": amount_usd,
                "asset": row.get("asset"),
                "chain": row.get("chain"),
                "direction": row.get("direction"),
                "source": row.get("source"),
            })
    return sorted(rows, key=lambda row: row["ts_ms"])


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def _bar_ms(candles):
    if len(candles) < 2:
        return 1
    return max(1, candles[1][0] - candles[0][0])


def burst_observations(events, candles, window_ms, horizon_bars, threshold, cooldown_bars):
    if window_ms <= 0 or horizon_bars <= 0 or threshold < 0 or cooldown_bars < 0:
        return []
    observations = []
    last_trigger_ts = None
    bar_ms = _bar_ms(candles)
    for index, (ts_ms, close) in enumerate(candles):
        if index + horizon_bars >= len(candles):
            break
        selected = [row for row in events if ts_ms - window_ms < row["ts_ms"] <= ts_ms]
        total = sum(row["amount_usd"] for row in selected)
        if total < threshold:
            continue
        if last_trigger_ts is not None and ts_ms - last_trigger_ts < cooldown_bars * bar_ms:
            continue
        future_close = candles[index + horizon_bars][1]
        forward = forward_return_pct(close, future_close)
        if forward is None:
            continue
        observations.append({
            "ts_ms": ts_ms,
            "window_amount_usd": total,
            "event_count": len(selected),
            "assets": sorted({str(row["asset"]).upper() for row in selected if row.get("asset")}),
            "chains": sorted({str(row["chain"]).lower() for row in selected if row.get("chain")}),
            "directions": sorted({str(row["direction"]).lower() for row in selected if row.get("direction")}),
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
            "horizon_ts_ms": candles[index + horizon_bars][0],
        })
        last_trigger_ts = ts_ms
    return observations


def summarize(observations, candles, horizon_bars, min_observations, min_abs_edge_bps):
    baseline = [
        abs(forward_return_pct(candles[index][1], candles[index + horizon_bars][1]))
        for index in range(max(0, len(candles) - horizon_bars))
        if forward_return_pct(candles[index][1], candles[index + horizon_bars][1]) is not None
    ]
    burst_moves = [row["forward_abs_return_pct"] for row in observations]
    burst_mean = statistics.mean(burst_moves) if burst_moves else None
    baseline_mean = statistics.mean(baseline) if baseline else None
    edge_bps = ((burst_mean - baseline_mean) * 100.0
                if burst_mean is not None and baseline_mean is not None else None)
    candidate = (len(observations) >= min_observations and edge_bps is not None
                 and edge_bps >= min_abs_edge_bps)
    return {
        "burst_observations": len(observations),
        "baseline_forward_windows": len(baseline),
        "mean_burst_abs_return_pct": burst_mean,
        "mean_baseline_abs_return_pct": baseline_mean,
        "absolute_move_edge_bps": edge_bps,
        "positive_forward_return_fraction": (
            sum(row["forward_return_pct"] > 0 for row in observations) / len(observations)
            if observations else None
        ),
        "median_window_amount_usd": (
            statistics.median(row["window_amount_usd"] for row in observations)
            if observations else None
        ),
        "verdict": "onchain_transfer_burst_move_candidate" if candidate else "observe_only",
        "evidence": [
            "rolling_onchain_transfer_window_available" if observations else "no_threshold_transfer_burst_observed",
            "forward_price_windows_available" if baseline else "missing_forward_price_windows",
            "transfer_burst_absolute_move_above_baseline" if candidate
            else "transfer_burst_edge_below_threshold_or_insufficient_observations",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--source", default=None)
    parser.add_argument("--chain", default=None)
    parser.add_argument("--asset", default="USDT")
    parser.add_argument("--min-transfer-usd", type=float, default=100_000.0)
    parser.add_argument("--price-exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--transfer-limit", type=int, default=500)
    parser.add_argument("--candle-limit", type=int, default=320)
    parser.add_argument("--window-hours", type=float, default=24.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--threshold-usd", type=float, default=1_000_000.0)
    parser.add_argument("--cooldown-bars", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.min_transfer_usd < 0 or args.transfer_limit <= 0 or args.candle_limit <= 0
            or args.window_hours <= 0 or args.horizon_bars <= 0 or args.threshold_usd < 0
            or args.cooldown_bars < 0 or args.min_observations <= 0 or args.min_abs_edge_bps < 0
            or args.timeout <= 0):
        parser.error("invalid transfer, candle, window, threshold, horizon or timeout arguments")
    transfer_payload = fetch(args.base_url, "/v1/onchain/transfers", {
        "source": args.source, "chain": args.chain, "asset": args.asset,
        "min_amount_usd": args.min_transfer_usd, "limit": min(args.transfer_limit, 5000),
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange, "symbol": args.symbol, "candle_type": "perp",
        "interval": args.interval, "limit": min(args.candle_limit, 1000),
    }, args.timeout)
    events = transfer_rows(transfer_payload)
    candles = candle_rows(candle_payload)
    observations = burst_observations(
        events, candles, int(args.window_hours * 3_600_000), args.horizon_bars,
        args.threshold_usd, args.cooldown_bars,
    )
    summary = summarize(observations, candles, args.horizon_bars,
                        args.min_observations, args.min_abs_edge_bps)
    evidence = list(summary.pop("evidence"))
    print(json.dumps({
        "strategy": "crypto_onchain_transfer_burst_replay",
        "source": args.source,
        "chain": args.chain,
        "asset": args.asset.upper(),
        "price_exchange": args.price_exchange,
        "symbol": args.symbol,
        "filters": {
            "min_transfer_usd": args.min_transfer_usd,
            "window_hours": args.window_hours,
            "horizon_bars": args.horizon_bars,
            "threshold_usd": args.threshold_usd,
            "cooldown_bars": args.cooldown_bars,
            "min_observations": args.min_observations,
            "min_abs_edge_bps": args.min_abs_edge_bps,
        },
        "events_scanned": len(events),
        "candle_points": len(candles),
        "observations": observations,
        "summary": summary,
        "evidence": evidence,
        "upstream_errors": [value for value in (
            transfer_payload.get("error"), candle_payload.get("error")
        ) if value],
        "limitations": [
            "transfer cache coverage is provider- and configuration-dependent, not a full-chain ledger",
            "direction and address labels are retained as metadata and are not interpreted as exchange net flow",
            "a transfer event can be part of an atomic bundle, treasury move, bridge or internal routing",
            "no fees, fills, gas, latency, wallet action, position sizing or directional trade model",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
