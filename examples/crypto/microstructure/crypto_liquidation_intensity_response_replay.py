#!/usr/bin/env python3
"""Replay responses after liquidation notional relative to candle turnover.

The study normalizes observed liquidation notional by an OHLCV quote-volume
proxy (typical price times base volume) over the same trailing candle window.
It compares high-intensity and ordinary windows by later absolute movement.
Provider event coverage, venue identity and side labels remain visible; this
is a non-directional risk-context replay, not a cascade forecast or execution
model.
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
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def candle_rows(payload):
    rows = []
    for row in payload.get("candles", []):
        values = {key: number(row.get(key)) for key in ("open", "high", "low", "close", "volume")}
        timestamp = row.get("open_time_ms")
        if (isinstance(timestamp, int) and all(value is not None for value in values.values())
                and values["open"] > 0 and values["close"] > 0
                and values["high"] >= values["low"] > 0 and values["volume"] >= 0
                and values["high"] >= values["open"] and values["high"] >= values["close"]
                and values["low"] <= values["open"] and values["low"] <= values["close"]):
            rows.append({"ts_ms": timestamp, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def liquidation_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        notional = number(row.get("notional"))
        if isinstance(timestamp, int) and notional is not None and notional > 0:
            rows.append({"ts_ms": timestamp, "notional": notional,
                         "side": str(row.get("side", "")).lower()})
    return sorted(rows, key=lambda row: row["ts_ms"])


def bar_width_ms(candles):
    gaps = [current["ts_ms"] - previous["ts_ms"]
            for previous, current in zip(candles, candles[1:])
            if current["ts_ms"] > previous["ts_ms"]]
    return int(statistics.median(gaps)) if gaps else None


def quote_volume(row):
    typical = (row["high"] + row["low"] + row["close"]) / 3.0
    return typical * row["volume"] if typical > 0 and row["volume"] >= 0 else None


def intensity_at(candles, events, index, window_bars):
    if window_bars <= 0 or index < window_bars - 1 or index >= len(candles):
        return None
    width = bar_width_ms(candles)
    if width is None:
        return None
    window = candles[index - window_bars + 1:index + 1]
    start_ts = window[0]["ts_ms"]
    end_ts = candles[index]["ts_ms"] + width
    selected = [event for event in events if start_ts <= event["ts_ms"] < end_ts]
    turnover = sum(quote_volume(row) or 0.0 for row in window)
    if turnover <= 0:
        return {"status": "missing_quote_volume", "window_bars": window_bars,
                "liquidation_notional": sum(event["notional"] for event in selected),
                "quote_volume": None, "event_count": len(selected)}
    liquidation_notional = sum(event["notional"] for event in selected)
    sell_notional = sum(event["notional"] for event in selected if event["side"] == "sell")
    buy_notional = sum(event["notional"] for event in selected if event["side"] == "buy")
    return {
        "status": "available",
        "window_bars": window_bars,
        "liquidation_notional": liquidation_notional,
        "quote_volume": turnover,
        "intensity_ratio": liquidation_notional / turnover,
        "event_count": len(selected),
        "sell_notional": sell_notional if sell_notional > 0 else None,
        "buy_notional": buy_notional if buy_notional > 0 else None,
        "sell_share": sell_notional / liquidation_notional if liquidation_notional > 0 else None,
        "buy_share": buy_notional / liquidation_notional if liquidation_notional > 0 else None,
    }


def classify_intensity(metrics, threshold, min_notional):
    if metrics is None or metrics.get("status") != "available":
        return "missing_quote_volume"
    if (metrics["liquidation_notional"] >= min_notional
            and metrics["intensity_ratio"] >= threshold):
        return "high_liquidation_intensity"
    return "ordinary_liquidation_intensity"


def build_observations(candles, events, window_bars, horizon_bars, threshold,
                       min_notional):
    if window_bars <= 0 or horizon_bars <= 0 or threshold < 0 or min_notional < 0:
        raise ValueError("invalid window, horizon, intensity or notional arguments")
    observations = []
    for index in range(window_bars - 1, len(candles) - horizon_bars):
        metrics = intensity_at(candles, events, index, window_bars)
        state = classify_intensity(metrics, threshold, min_notional)
        if metrics is None:
            continue
        future = candles[index + horizon_bars]
        forward = (future["close"] / candles[index]["close"] - 1.0) * 100.0
        observations.append({
            "ts_ms": candles[index]["ts_ms"],
            "future_ts_ms": future["ts_ms"],
            "state": state,
            "metrics": metrics,
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
        })
    return observations


def bucket_stats(rows):
    signed = [row["forward_return_pct"] for row in rows]
    absolute = [row["forward_abs_return_pct"] for row in rows]
    return {
        "observations": len(rows),
        "mean_forward_return_pct": statistics.mean(signed) if signed else None,
        "median_forward_return_pct": statistics.median(signed) if signed else None,
        "mean_absolute_return_pct": statistics.mean(absolute) if absolute else None,
        "median_intensity_ratio": (statistics.median(row["metrics"]["intensity_ratio"] for row in rows
                                                       if row["metrics"].get("intensity_ratio") is not None)
                                    if any(row["metrics"].get("intensity_ratio") is not None for row in rows)
                                    else None),
    }


def summarize(observations, min_observations, min_abs_edge_bps):
    states = ("high_liquidation_intensity", "ordinary_liquidation_intensity", "missing_quote_volume")
    by_state = {state: bucket_stats([row for row in observations if row["state"] == state])
                for state in states}
    high = by_state["high_liquidation_intensity"]
    ordinary = by_state["ordinary_liquidation_intensity"]
    edge_bps = ((high["mean_absolute_return_pct"] - ordinary["mean_absolute_return_pct"]) * 100.0
                if high["mean_absolute_return_pct"] is not None
                and ordinary["mean_absolute_return_pct"] is not None else None)
    sufficient = (high["observations"] >= min_observations
                  and ordinary["observations"] >= min_observations)
    candidate = sufficient and edge_bps is not None and edge_bps >= min_abs_edge_bps
    return {
        "observations": len(observations),
        "high_intensity_observations": high["observations"],
        "ordinary_observations": ordinary["observations"],
        "missing_quote_volume_observations": by_state["missing_quote_volume"]["observations"],
        "absolute_move_edge_bps_high_minus_ordinary": edge_bps,
        "by_state": by_state,
        "min_observations": min_observations,
        "min_abs_edge_bps": min_abs_edge_bps,
        "verdict": ("liquidation_intensity_response_reported" if candidate
                     else "observe_only_insufficient_intensity_or_control"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--liquidation-exchange", choices=("okx", "coinex"), default="okx")
    parser.add_argument("--price-exchange", choices=("okx", "binance"), default="okx")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--liquidation-limit", type=int, default=100)
    parser.add_argument("--candle-limit", type=int, default=320)
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--window-bars", type=int, default=12)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--min-intensity-ratio", type=float, default=0.01)
    parser.add_argument("--min-notional", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.liquidation_limit <= 0 or args.candle_limit <= 0 or args.window_bars <= 0
            or args.horizon_bars <= 0 or args.min_intensity_ratio < 0 or args.min_notional < 0
            or args.min_observations <= 0 or args.min_abs_edge_bps < 0 or args.timeout <= 0):
        parser.error("invalid limits, window, intensity, horizon or observation arguments")
    liquidation_payload = fetch(args.base_url, "/v1/history/liquidations", {
        "exchange": args.liquidation_exchange, "symbol": args.symbol,
        "limit": min(args.liquidation_limit, 100),
    }, args.timeout)
    candle_payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.price_exchange, "symbol": args.symbol,
        "market": "perp", "candle_type": "perp", "interval": args.interval,
        "limit": min(args.candle_limit, 1000),
    }, args.timeout)
    events = liquidation_rows(liquidation_payload)
    candles = candle_rows(candle_payload)
    observations = build_observations(
        candles, events, args.window_bars, args.horizon_bars,
        args.min_intensity_ratio, args.min_notional,
    )
    print(json.dumps({
        "strategy": "crypto_liquidation_intensity_response_replay",
        "hypothesis": "liquidation notional relative to same-window OHLCV quote turnover may separate later absolute movement from ordinary windows",
        "market": {"liquidation_exchange": args.liquidation_exchange,
                   "price_exchange": args.price_exchange, "symbol": args.symbol,
                   "market": "perp", "interval": args.interval, "timezone": "UTC"},
        "filters": {"window_bars": args.window_bars, "horizon_bars": args.horizon_bars,
                    "min_intensity_ratio": args.min_intensity_ratio,
                    "min_notional": args.min_notional,
                    "min_observations": args.min_observations,
                    "min_abs_edge_bps": args.min_abs_edge_bps},
        "source_counts": {"liquidation_events": len(events), "price_bars": len(candles),
                          "observations": len(observations)},
        "observations": observations,
        "summary": summarize(observations, args.min_observations, args.min_abs_edge_bps),
        "coverage": {"liquidations": liquidation_payload.get("coverage_detail"),
                     "candles": candle_payload.get("coverage_detail")},
        "upstream_errors": [value for value in (liquidation_payload.get("error"),
                                                  candle_payload.get("error")) if value],
        "limitations": [
            "liquidation notional and candle quote volume are provider-normalized proxies and may come from different venues",
            "candle quote volume is typical-price times base volume, not a tick-level traded-dollar ledger",
            "bounded liquidation history can omit events and cannot be treated as a complete cascade record",
            "side labels are retained as provider metadata and are not interpreted as long/short liquidation truth",
            "row-count horizons omit missing-bar timing, fees, funding, slippage, queue and fills",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
