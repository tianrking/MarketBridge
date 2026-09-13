#!/usr/bin/env python3
"""Replay an event-anchored VWAP reclaim/rejection hypothesis.

At each candle the anchor is chosen only from the preceding lookback window:
the prior swing low tests a bullish reclaim, while the prior swing high tests a
bearish rejection.  A crossing and optional volume confirmation are scored
against a later fixed candle index.  This is a bounded OHLCV study, never an
entry, order or execution engine.
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
        timestamp = row.get("open_time_ms")
        close, high, low, volume = (number(row.get(key)) for key in ("close", "high", "low", "volume"))
        if (isinstance(timestamp, int) and close is not None and high is not None
                and low is not None and close > 0 and high > 0 and low > 0):
            rows.append({"ts_ms": timestamp, "close": close, "high": high, "low": low,
                         "volume": volume if volume is not None and volume >= 0 else None})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def anchored_value(rows, anchor_index, end_index):
    selected = rows[anchor_index:end_index + 1]
    weighted = sum(((row["high"] + row["low"] + row["close"]) / 3.0) * row["volume"]
                   for row in selected if row["volume"] is not None)
    volume = sum(row["volume"] for row in selected if row["volume"] is not None)
    return weighted / volume if volume > 0 else None


def anchor_index(rows, index, lookback, anchor_type):
    if index < lookback:
        return None
    start = index - lookback
    window = range(start, index)
    if anchor_type == "low":
        return min(window, key=lambda item: (rows[item]["low"], rows[item]["ts_ms"]))
    if anchor_type == "high":
        return max(window, key=lambda item: (rows[item]["high"], -rows[item]["ts_ms"]))
    raise ValueError(f"unsupported anchor type: {anchor_type}")


def anchored_vwap_observations(rows, anchor_lookback, anchor_mode, cross_buffer_bps,
                               volume_multiplier, horizon_bars):
    output = []
    anchor_types = ("low", "high") if anchor_mode == "both" else (anchor_mode,)
    for index in range(anchor_lookback + 1, max(anchor_lookback + 1, len(rows) - horizon_bars)):
        for anchor_type in anchor_types:
            selected_anchor = anchor_index(rows, index, anchor_lookback, anchor_type)
            current_vwap = anchored_value(rows, selected_anchor, index)
            previous_vwap = anchored_value(rows, selected_anchor, index - 1)
            if current_vwap is None or previous_vwap is None:
                continue
            previous_close = rows[index - 1]["close"]
            close = rows[index]["close"]
            buffer = cross_buffer_bps / 10_000.0
            if anchor_type == "low":
                crossed = previous_close <= previous_vwap and close > current_vwap * (1.0 + buffer)
                direction = 1
            else:
                crossed = previous_close >= previous_vwap and close < current_vwap * (1.0 - buffer)
                direction = -1
            if not crossed:
                continue
            prior_volumes = [row["volume"] for row in rows[index - anchor_lookback:index]
                             if row["volume"] is not None and row["volume"] >= 0]
            average_volume = statistics.mean(prior_volumes) if prior_volumes else None
            volume = rows[index]["volume"]
            volume_confirmed = (volume is not None and average_volume is not None
                                and volume >= average_volume * volume_multiplier)
            if volume_multiplier > 0 and not volume_confirmed:
                continue
            future = rows[index + horizon_bars]
            forward_return = (future["close"] / close - 1.0) * 100.0
            aligned = direction * forward_return
            output.append({
                "ts_ms": rows[index]["ts_ms"],
                "forward_ts_ms": future["ts_ms"],
                "anchor_ts_ms": rows[selected_anchor]["ts_ms"],
                "anchor_type": anchor_type,
                "direction": "long" if direction > 0 else "short",
                "anchor_price": rows[selected_anchor]["low" if anchor_type == "low" else "high"],
                "anchored_vwap": current_vwap,
                "distance_from_vwap_bps": (close / current_vwap - 1.0) * 10_000,
                "volume": volume,
                "average_volume": average_volume,
                "volume_confirmed": volume_confirmed,
                "forward_return_pct": forward_return,
                "aligned_return_bps": aligned * 100.0,
                "aligned": aligned > 0,
            })
    return output


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    gross = [row["aligned_return_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    by_anchor = {}
    for anchor_type in sorted({row["anchor_type"] for row in observations}):
        values = [row["aligned_return_bps"] for row in observations if row["anchor_type"] == anchor_type]
        by_anchor[anchor_type] = {
            "signals": len(values),
            "mean_aligned_return_bps": statistics.mean(values) if values else None,
            "hit_rate": sum(value > 0 for value in values) / len(values) if values else None,
        }
    return {
        "signals": len(observations),
        "aligned_signals": sum(row["aligned"] for row in observations),
        "hit_rate": sum(row["aligned"] for row in observations) / len(observations) if observations else None,
        "mean_aligned_return_bps": statistics.mean(gross) if gross else None,
        "median_aligned_return_bps": statistics.median(gross) if gross else None,
        "paper_cost_bps": paper_cost_bps,
        "mean_cost_adjusted_return_bps": mean_adjusted,
        "min_edge_bps": min_edge_bps,
        "by_anchor": by_anchor,
        "verdict": "anchored_vwap_response_candidate" if candidate else "observe_only",
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="5m")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--anchor-lookback", type=int, default=96)
    parser.add_argument("--anchor-mode", choices=("low", "high", "both"), default="both")
    parser.add_argument("--cross-buffer-bps", type=float, default=2.0)
    parser.add_argument("--volume-multiplier", type=float, default=1.0)
    parser.add_argument("--horizon-bars", type=int, default=12)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or not 1 <= args.limit <= 1500 or args.anchor_lookback < 2
            or args.cross_buffer_bps < 0 or args.volume_multiplier < 0 or args.horizon_bars <= 0
            or args.paper_cost_bps < 0 or args.min_edge_bps < 0 or args.min_observations <= 0
            or args.timeout <= 0):
        parser.error("invalid anchor, volume, horizon, cost or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": args.limit,
    }, args.timeout)
    rows = candle_rows(payload)
    observations = anchored_vwap_observations(
        rows, args.anchor_lookback, args.anchor_mode, args.cross_buffer_bps,
        args.volume_multiplier, args.horizon_bars,
    )
    summary = summarize(observations, args.min_observations, args.paper_cost_bps, args.min_edge_bps)
    coverage = payload.get("coverage_detail")
    evidence = ["anchored_vwap_crossings_and_forward_returns_available" if observations
                else "no_qualifying_anchored_vwap_crossings"]
    if isinstance(coverage, dict) and coverage.get("status"):
        evidence.append(f"candle_coverage_{coverage['status']}")
    print(json.dumps({
        "strategy": "crypto_anchored_vwap_replay",
        "market": {"exchange": args.exchange, "market": args.market,
                   "symbol": args.symbol, "interval": args.interval},
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"anchor_lookback": args.anchor_lookback, "anchor_mode": args.anchor_mode,
                    "cross_buffer_bps": args.cross_buffer_bps,
                    "volume_multiplier": args.volume_multiplier,
                    "horizon_bars": args.horizon_bars, "paper_cost_bps": args.paper_cost_bps,
                    "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
        "source_counts": {"candles": len(rows)},
        "observations": observations,
        "summary": summary,
        "coverage": coverage,
        "evidence": evidence,
        "upstream_errors": payload.get("errors", []),
        "limitations": [
            "swing anchors are selected from prior OHLCV extremes, not an externally identified event",
            "anchored VWAP uses typical-price OHLCV volume, not tick-level execution flow",
            "fixed candle-index horizon is not a fill, latency, fee, funding or slippage model",
            "crossing and volume thresholds are sensitivity parameters, not universal signals",
            "no order, wallet, allocation or execution path is included",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
