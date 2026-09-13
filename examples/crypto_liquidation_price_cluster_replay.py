#!/usr/bin/env python3
"""Replay whether observed liquidation price clusters precede larger moves.

Public heatmaps often describe dense liquidation bands as pressure points.  This
example does not reconstruct latent liquidation levels.  It only clusters
MarketBridge's observed, executed liquidation prints by price inside a rolling
window and tests whether a concentrated window is followed by a larger
absolute perp return than ordinary windows.  Side labels remain metadata.
"""

import argparse
import json
import math
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
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            rows.append((timestamp, close))
    return sorted(set(rows))


def liquidation_rows(payload):
    rows = []
    for row in payload.get("rows", []):
        timestamp = row.get("ts_ms")
        price = number(row.get("price"))
        notional = number(row.get("notional"))
        if (isinstance(timestamp, int) and price is not None and price > 0
                and notional is not None and notional > 0):
            rows.append({
                "ts_ms": timestamp,
                "price": price,
                "notional": notional,
                "side": row.get("side"),
            })
    return sorted(rows, key=lambda row: row["ts_ms"])


def _bucket(price, anchor, band_bps):
    scaled = math.log(price / anchor) * 10_000.0 / band_bps
    return math.floor(scaled + 0.5)


def cluster_profile(events, band_bps):
    """Return the dominant observed-price band and its notional share."""
    if not events or band_bps <= 0:
        return None
    anchor = statistics.median(row["price"] for row in events)
    buckets = {}
    for row in events:
        key = _bucket(row["price"], anchor, band_bps)
        bucket = buckets.setdefault(key, {"notional": 0.0, "weighted_price": 0.0, "events": 0})
        bucket["notional"] += row["notional"]
        bucket["weighted_price"] += row["price"] * row["notional"]
        bucket["events"] += 1
    key, selected = max(buckets.items(), key=lambda item: item[1]["notional"])
    total = sum(bucket["notional"] for bucket in buckets.values())
    if total <= 0:
        return None
    return {
        "bucket": key,
        "center_price": selected["weighted_price"] / selected["notional"],
        "cluster_notional": selected["notional"],
        "total_notional": total,
        "cluster_share": selected["notional"] / total,
        "cluster_events": selected["events"],
        "price_band_bps": band_bps,
    }


def forward_return_pct(current, future):
    if current is None or future is None or current <= 0 or future <= 0:
        return None
    return (future / current - 1.0) * 100.0


def cluster_observations(events, candles, window_ms, horizon_bars, threshold,
                         band_bps, min_share, cooldown_bars):
    if (window_ms <= 0 or horizon_bars <= 0 or threshold < 0 or band_bps <= 0
            or not 0.0 <= min_share <= 1.0 or cooldown_bars < 0):
        return []
    observations = []
    last_trigger_ts = None
    bar_ms = _bar_ms(candles)
    for index, (timestamp, close) in enumerate(candles):
        if index + horizon_bars >= len(candles):
            break
        selected = [row for row in events if timestamp - window_ms < row["ts_ms"] <= timestamp]
        profile = cluster_profile(selected, band_bps)
        if profile is None or profile["total_notional"] < threshold:
            continue
        if profile["cluster_share"] < min_share:
            continue
        if (last_trigger_ts is not None
                and timestamp - last_trigger_ts < cooldown_bars * bar_ms):
            continue
        future_close = candles[index + horizon_bars][1]
        forward = forward_return_pct(close, future_close)
        if forward is None:
            continue
        profile.update({
            "ts_ms": timestamp,
            "current_price": close,
            "distance_to_cluster_bps": (profile["center_price"] / close - 1.0) * 10_000.0,
            "forward_return_pct": forward,
            "forward_abs_return_pct": abs(forward),
            "horizon_ts_ms": candles[index + horizon_bars][0],
        })
        observations.append(profile)
        last_trigger_ts = timestamp
    return observations


def _bar_ms(candles):
    if len(candles) < 2:
        return 1
    return max(1, candles[1][0] - candles[0][0])


def summarize(observations, candles, horizon_bars, min_observations, min_abs_edge_bps):
    baseline = [
        abs(forward_return_pct(candles[index][1], candles[index + horizon_bars][1]))
        for index in range(max(0, len(candles) - horizon_bars))
        if forward_return_pct(candles[index][1], candles[index + horizon_bars][1]) is not None
    ]
    cluster_moves = [row["forward_abs_return_pct"] for row in observations]
    cluster_mean = statistics.mean(cluster_moves) if cluster_moves else None
    baseline_mean = statistics.mean(baseline) if baseline else None
    edge_bps = ((cluster_mean - baseline_mean) * 100.0
                if cluster_mean is not None and baseline_mean is not None else None)
    candidate = (len(observations) >= min_observations and edge_bps is not None
                 and edge_bps >= min_abs_edge_bps)
    return {
        "cluster_observations": len(observations),
        "baseline_forward_windows": len(baseline),
        "mean_cluster_abs_return_pct": cluster_mean,
        "mean_baseline_abs_return_pct": baseline_mean,
        "absolute_move_edge_bps": edge_bps,
        "median_cluster_share": (
            statistics.median(row["cluster_share"] for row in observations)
            if observations else None
        ),
        "verdict": "liquidation_price_cluster_candidate" if candidate else "observe_only",
        "evidence": [
            "observed_liquidation_price_bands_available" if observations
            else "no_concentrated_liquidation_band_observed",
            "forward_price_windows_available" if baseline else "missing_forward_price_windows",
            "cluster_absolute_move_above_baseline" if candidate
            else "cluster_edge_below_threshold_or_insufficient_observations",
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
    parser.add_argument("--cluster-band-bps", type=float, default=25.0)
    parser.add_argument("--min-cluster-share", type=float, default=0.5)
    parser.add_argument("--cooldown-bars", type=int, default=12)
    parser.add_argument("--min-observations", type=int, default=3)
    parser.add_argument("--min-abs-edge-bps", type=float, default=0.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.liquidation_limit <= 0 or args.candle_limit <= 0 or args.window_hours <= 0
            or args.horizon_bars <= 0 or args.threshold_notional < 0
            or args.cluster_band_bps <= 0 or not 0.0 <= args.min_cluster_share <= 1.0
            or args.cooldown_bars < 0 or args.min_observations <= 0
            or args.min_abs_edge_bps < 0):
        parser.error("invalid limits, window, cluster, threshold, horizon or observation arguments")
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
    observations = cluster_observations(
        events, candles, int(args.window_hours * 3_600_000), args.horizon_bars,
        args.threshold_notional, args.cluster_band_bps, args.min_cluster_share,
        args.cooldown_bars,
    )
    coverage = liquidation_payload.get("coverage_detail")
    summary = summarize(observations, candles, args.horizon_bars,
                        args.min_observations, args.min_abs_edge_bps)
    evidence = list(summary.pop("evidence"))
    if coverage:
        evidence.append(f"liquidation_coverage_{coverage.get('status', 'reported')}")
    print(json.dumps({
        "strategy": "crypto_liquidation_price_cluster_replay",
        "exchange": args.exchange,
        "price_exchange": price_exchange,
        "symbol": args.symbol,
        "filters": {
            "window_hours": args.window_hours,
            "horizon_bars": args.horizon_bars,
            "threshold_notional": args.threshold_notional,
            "cluster_band_bps": args.cluster_band_bps,
            "min_cluster_share": args.min_cluster_share,
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
            "observed liquidation prints are not a forecast of untouched liquidation levels",
            "price bands depend on the selected window and cluster bandwidth",
            "side labels are metadata and are not interpreted as long/short truth",
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
