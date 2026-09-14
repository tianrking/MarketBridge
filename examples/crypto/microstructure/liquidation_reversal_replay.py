#!/usr/bin/env python3
"""Replay a bounded liquidation-flush / price-recovery hypothesis.

This uses public OKX or CoinEx liquidation history plus OHLCV candles, with
optional Binance/Bybit/OKX OI and Binance/OKX trade joins. A sell-side liquidation
is treated as a long-liquidation proxy, then the script measures the subsequent
candle return. Any unrequested or unavailable context remains explicit, so this
is a partial paper replay rather than a profitability backtest.
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
    return float(value) if isinstance(value, (int, float)) else None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        ts_ms = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(ts_ms, int) and close is not None and close > 0:
            rows.append((ts_ms, close))
    return sorted(rows)


def event_recovery(event, candles, horizon_bars):
    event_ts = event.get("ts_ms")
    price = number(event.get("price"))
    if not isinstance(event_ts, int) or price is None or price <= 0:
        return None
    after = [row for row in candles if row[0] >= event_ts]
    if len(after) <= horizon_bars:
        return None
    entry_price = after[0][1]
    exit_price = after[horizon_bars][1]
    return (exit_price - entry_price) / entry_price * 100.0


def oi_change_at_event(event, oi_points):
    event_ts = event.get("ts_ms")
    if not isinstance(event_ts, int):
        return None
    before = [point for point in oi_points if point[0] <= event_ts]
    after = [point for point in oi_points if point[0] > event_ts]
    if not before or not after or before[-1][1] <= 0:
        return None
    return (after[0][1] - before[-1][1]) / before[-1][1] * 100.0


def cvd_at_event(event, trade_rows, window_ms):
    event_ts = event.get("ts_ms")
    if not isinstance(event_ts, int):
        return None
    end_ts = event_ts + window_ms
    selected = [row for row in trade_rows if event_ts <= row.get("ts_ms", -1) <= end_ts]
    if not selected:
        return None
    return sum(
        (1.0 if str(row.get("side", "")).lower() == "buy" else -1.0)
        * number(row.get("notional"))
        for row in selected
        if number(row.get("notional")) is not None
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", choices=("okx", "coinex"), default="okx")
    parser.add_argument(
        "--price-exchange",
        choices=("okx", "binance"),
        default=None,
        help="venue for OHLCV context; defaults to the liquidation venue when supported, otherwise okx",
    )
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--candle-limit", type=int, default=300)
    parser.add_argument("--horizon-bars", type=int, default=3)
    parser.add_argument("--min-notional", type=float, default=100_000.0)
    parser.add_argument("--min-recovery-pct", type=float, default=0.1)
    parser.add_argument(
        "--oi-exchange",
        choices=("none", "binance", "bybit"),
        default="none",
        help="optional public historical OI context; it may be a different venue from liquidation events",
    )
    parser.add_argument(
        "--trades-exchange",
        choices=("none", "binance", "okx"),
        default="none",
        help="optional public historical trade context for CVD",
    )
    parser.add_argument("--cvd-window-ms", type=int, default=900_000)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if options.limit <= 0 or options.candle_limit <= 0 or options.horizon_bars <= 0:
        raise SystemExit("limits and horizon-bars must be positive")
    if options.min_notional < 0 or options.cvd_window_ms <= 0:
        raise SystemExit("min-notional cannot be negative and cvd-window-ms must be positive")

    price_exchange = options.price_exchange or (
        options.exchange if options.exchange in ("okx", "binance") else "okx"
    )
    liquidation_payload = fetch(options.base_url, "/v1/history/liquidations", {
        "exchange": options.exchange,
        "symbol": options.symbol,
        "limit": min(options.limit, 100),
    }, options.timeout)
    candle_payload = fetch(options.base_url, "/v1/history/candles", {
        "exchange": price_exchange,
        "symbol": options.symbol,
        "candle_type": "perp",
        "interval": "5m",
        "limit": min(options.candle_limit, 300),
    }, options.timeout)
    candles = candle_rows(candle_payload)
    oi_points = []
    oi_payload = {}
    if options.oi_exchange != "none":
        oi_payload = fetch(options.base_url, "/v1/history/open-interest", {
            "exchange": options.oi_exchange,
            "symbol": options.symbol,
            "interval": "5m",
            "limit": 200,
        }, options.timeout)
        oi_points = sorted(
            (row["ts_ms"], number(row.get("open_interest")))
            for row in oi_payload.get("rows", [])
            if isinstance(row, dict)
            and isinstance(row.get("ts_ms"), int)
            and number(row.get("open_interest")) is not None
        )
    trade_payload = {}
    trade_rows = []
    if options.trades_exchange != "none":
        trade_payload = fetch(options.base_url, "/v1/history/trades", {
            "exchange": options.trades_exchange,
            "symbol": options.symbol,
            "limit": 1000 if options.trades_exchange == "binance" else 100,
        }, options.timeout)
        trade_rows = trade_payload.get("rows", [])
    events = [
        event for event in liquidation_payload.get("rows", [])
        if str(event.get("side", "")).lower() == "sell"
        and number(event.get("notional")) is not None
        and number(event.get("notional")) >= options.min_notional
    ]
    recoveries = []
    candidates = []
    for event in events:
        recovery = event_recovery(event, candles, options.horizon_bars)
        if recovery is None:
            continue
        oi_change_pct = oi_change_at_event(event, oi_points) if oi_points else None
        cvd_notional = cvd_at_event(event, trade_rows, options.cvd_window_ms) if trade_rows else None
        row = {
            "ts_ms": event.get("ts_ms"),
            "notional": event.get("notional"),
            "price": event.get("price"),
            "recovery_pct": recovery,
            "oi_change_pct": oi_change_pct,
            "oi_falling": oi_change_pct is not None and oi_change_pct < 0.0,
            "cvd_notional": cvd_notional,
            "cvd_positive": cvd_notional is not None and cvd_notional > 0.0,
            "position_side": event.get("position_side"),
        }
        recoveries.append(row)
        if recovery >= options.min_recovery_pct:
            candidates.append(row)

    evidence = []
    if events:
        evidence.append("sell_side_liquidation_events_available")
    else:
        evidence.append("no_sell_side_liquidation_above_threshold")
    liquidation_coverage = liquidation_payload.get("coverage_detail")
    if liquidation_coverage:
        evidence.append(f"liquidation_coverage_{liquidation_coverage.get('status', 'reported')}")
    if recoveries:
        evidence.append("forward_price_window_available")
    else:
        evidence.append("no_forward_price_window_for_selected_events")
    print(json.dumps({
        "strategy": "liquidation_reversal_replay",
        "exchange": options.exchange,
        "price_exchange": price_exchange,
        "symbol": options.symbol,
        "filters": {
            "min_notional": options.min_notional,
            "horizon_bars": options.horizon_bars,
            "min_recovery_pct": options.min_recovery_pct,
            "oi_exchange": options.oi_exchange,
            "trades_exchange": options.trades_exchange,
            "cvd_window_ms": options.cvd_window_ms,
        },
        "events_scanned": len(events),
        "liquidation_coverage": liquidation_coverage,
        "recovery_observations": len(recoveries),
        "candidates": candidates,
        "summary": {
            "mean_recovery_pct": statistics.mean(row["recovery_pct"] for row in recoveries) if recoveries else None,
            "median_recovery_pct": statistics.median(row["recovery_pct"] for row in recoveries) if recoveries else None,
            "candidate_fraction": len(candidates) / len(recoveries) if recoveries else None,
            "oi_joined_observations": sum(row["oi_change_pct"] is not None for row in recoveries),
            "oi_falling_fraction": (
                sum(row["oi_falling"] for row in recoveries if row["oi_change_pct"] is not None)
                / sum(row["oi_change_pct"] is not None for row in recoveries)
                if any(row["oi_change_pct"] is not None for row in recoveries)
                else None
            ),
            "cvd_joined_observations": sum(row["cvd_notional"] is not None for row in recoveries),
            "cvd_positive_fraction": (
                sum(row["cvd_positive"] for row in recoveries if row["cvd_notional"] is not None)
                / sum(row["cvd_notional"] is not None for row in recoveries)
                if any(row["cvd_notional"] is not None for row in recoveries)
                else None
            ),
        },
        "verdict": "partial research candidate" if candidates else "observe only",
        "evidence": evidence,
        "missing_context": [
            *(["historical CVD/order-flow context not requested"] if options.trades_exchange == "none" else []),
            "no fill, fee, slippage, latency or position-sizing model",
        ],
        "upstream_errors": [
            value for value in (
                liquidation_payload.get("error"),
                candle_payload.get("error"),
                oi_payload.get("error") if oi_payload else None,
                trade_payload.get("error") if trade_payload else None,
            ) if value
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
