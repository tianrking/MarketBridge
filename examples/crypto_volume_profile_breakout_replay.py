#!/usr/bin/env python3
"""Replay a volume-profile low-volume-node breakout hypothesis.

The profile is an OHLCV approximation: each candle's typical price receives
the candle's full volume.  The falsifiable question is whether a close leaving
the prior value area into a low-volume bin, with volume confirmation, is
followed by a directional fixed-horizon return.  It is not a tick-level
volume profile, order-book map or execution model.
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
        values = {key: number(row.get(key)) for key in ("close", "high", "low", "volume")}
        if (isinstance(ts_ms, int) and values["close"] is not None and values["high"] is not None
                and values["low"] is not None and values["close"] > 0):
            rows.append({"ts_ms": ts_ms, **values})
    return sorted({row["ts_ms"]: row for row in rows}.values(), key=lambda row: row["ts_ms"])


def percentile(values, fraction):
    ordered = sorted(values)
    if not ordered:
        return None
    position = (len(ordered) - 1) * fraction
    lower, upper = int(position), min(int(position) + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def profile_features(rows, index, lookback_bars, bins, value_area_fraction, low_volume_quantile):
    if index < lookback_bars or bins < 2:
        return None
    prior = rows[index - lookback_bars:index]
    low = min(row["low"] for row in prior)
    high = max(row["high"] for row in prior)
    if not high > low:
        return None
    width = (high - low) / bins
    volumes = [0.0] * bins
    for row in prior:
        price = (row["high"] + row["low"] + row["close"]) / 3.0
        bucket = min(bins - 1, max(0, int((price - low) / width)))
        volumes[bucket] += max(0.0, row["volume"] or 0.0)
    total = sum(volumes)
    if total <= 0:
        return None
    poc = max(range(bins), key=volumes.__getitem__)
    selected = {poc}
    covered = volumes[poc]
    while covered < total * value_area_fraction and len(selected) < bins:
        candidates = [candidate for candidate in (min(selected) - 1, max(selected) + 1)
                      if 0 <= candidate < bins and candidate not in selected]
        if not candidates:
            break
        candidate = max(candidates, key=volumes.__getitem__)
        selected.add(candidate)
        covered += volumes[candidate]
    value_low = low + min(selected) * width
    value_high = low + (max(selected) + 1) * width
    threshold = percentile(volumes, low_volume_quantile)
    low_nodes = [bucket for bucket, volume in enumerate(volumes) if volume <= threshold]
    current_price = rows[index]["close"]
    current_bucket = min(bins - 1, max(0, int((current_price - low) / width)))
    return {
        "profile_low": low, "profile_high": high, "poc": low + (poc + 0.5) * width,
        "value_low": value_low, "value_high": value_high,
        "current_bucket": current_bucket, "low_volume_threshold": threshold,
        "current_in_low_node": current_bucket in low_nodes,
        "low_node_count": len(low_nodes), "volume_total": total,
    }


def breakout_observations(rows, lookback_bars, bins, value_area_fraction,
                          low_volume_quantile, breakout_buffer, volume_multiplier,
                          horizon_bars):
    by_ts = {row["ts_ms"]: row for row in rows}
    output = []
    for index, row in enumerate(rows):
        profile = profile_features(rows, index, lookback_bars, bins,
                                   value_area_fraction, low_volume_quantile)
        if profile is None or index == 0:
            continue
        prior_close = rows[index - 1]["close"]
        direction = 1 if (prior_close <= profile["value_high"]
                          and row["close"] > profile["value_high"] * (1.0 + breakout_buffer)) else (
            -1 if (prior_close >= profile["value_low"]
                   and row["close"] < profile["value_low"] * (1.0 - breakout_buffer)) else 0)
        if direction == 0 or not profile["current_in_low_node"]:
            continue
        previous_volumes = [item["volume"] for item in rows[index - lookback_bars:index]
                            if item["volume"] is not None and item["volume"] >= 0]
        average_volume = statistics.mean(previous_volumes) if previous_volumes else None
        if (average_volume is None or row["volume"] is None
                or row["volume"] < average_volume * volume_multiplier):
            continue
        forward_ts = row["ts_ms"] + horizon_bars * 60_000
        future = by_ts.get(forward_ts)
        if future is None:
            continue
        forward_return_pct = (future["close"] / row["close"] - 1.0) * 100.0
        aligned = direction * forward_return_pct
        output.append({"ts_ms": row["ts_ms"], "forward_ts_ms": forward_ts,
                       "direction": "long" if direction > 0 else "short",
                       "profile": profile, "volume": row["volume"],
                       "average_volume": average_volume, "forward_return_pct": forward_return_pct,
                       "aligned_return_bps": aligned * 100.0, "aligned": aligned > 0})
    return output


def summarize(observations, min_observations, paper_cost_bps, min_edge_bps):
    gross = [row["aligned_return_bps"] for row in observations]
    adjusted = [value - paper_cost_bps for value in gross]
    mean_adjusted = statistics.mean(adjusted) if adjusted else None
    candidate = (len(adjusted) >= min_observations and mean_adjusted is not None
                 and mean_adjusted >= min_edge_bps)
    return {"signals": len(observations), "aligned_signals": sum(row["aligned"] for row in observations),
            "hit_rate": (sum(row["aligned"] for row in observations) / len(observations)
                         if observations else None),
            "mean_aligned_return_bps": statistics.mean(gross) if gross else None,
            "paper_cost_bps": paper_cost_bps, "mean_cost_adjusted_return_bps": mean_adjusted,
            "min_edge_bps": min_edge_bps,
            "verdict": "low_volume_node_breakout_candidate" if candidate else "observe_only",
            "research_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--market", default="perp")
    parser.add_argument("--interval", default="1m")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--lookback-bars", type=int, default=120)
    parser.add_argument("--bins", type=int, default=24)
    parser.add_argument("--value-area-fraction", type=float, default=0.70)
    parser.add_argument("--low-volume-quantile", type=float, default=0.25)
    parser.add_argument("--breakout-buffer-bps", type=float, default=2.0)
    parser.add_argument("--volume-multiplier", type=float, default=1.2)
    parser.add_argument("--horizon-bars", type=int, default=30)
    parser.add_argument("--paper-cost-bps", type=float, default=0.0)
    parser.add_argument("--min-edge-bps", type=float, default=0.0)
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.limit < args.lookback_bars or args.lookback_bars < 2
            or args.bins < 2 or not 0 < args.value_area_fraction <= 1
            or not 0 <= args.low_volume_quantile <= 1 or args.breakout_buffer_bps < 0
            or args.volume_multiplier < 0 or args.horizon_bars <= 0 or args.paper_cost_bps < 0
            or args.min_edge_bps < 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("invalid profile, breakout, volume, horizon or observation arguments")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    payload = fetch(args.base_url, "/v1/history/candles", {
        "exchange": args.exchange, "market": args.market, "symbol": args.symbol,
        "interval": args.interval, "start_ms": start_ms, "end_ms": end_ms,
        "limit": min(args.limit, 1500),
    }, args.timeout)
    rows = candle_rows(payload)
    observations = breakout_observations(
        rows, args.lookback_bars, args.bins, args.value_area_fraction,
        args.low_volume_quantile, args.breakout_buffer_bps / 10_000.0,
        args.volume_multiplier, args.horizon_bars,
    )
    summary = summarize(observations, args.min_observations, args.paper_cost_bps, args.min_edge_bps)
    evidence = ["profile_breakout_and_forward_returns_available" if observations else "no_qualifying_low_volume_node_breakouts"]
    detail = payload.get("coverage_detail")
    if isinstance(detail, dict) and detail.get("status"):
        evidence.append(f"candle_coverage_{detail['status']}")
    print(json.dumps({"strategy": "crypto_volume_profile_breakout_replay",
                      "market": {"exchange": args.exchange, "market": args.market,
                                 "symbol": args.symbol, "interval": args.interval},
                      "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
                      "filters": {"lookback_bars": args.lookback_bars, "bins": args.bins,
                                  "value_area_fraction": args.value_area_fraction,
                                  "low_volume_quantile": args.low_volume_quantile,
                                  "breakout_buffer_bps": args.breakout_buffer_bps,
                                  "volume_multiplier": args.volume_multiplier,
                                  "horizon_bars": args.horizon_bars, "paper_cost_bps": args.paper_cost_bps,
                                  "min_edge_bps": args.min_edge_bps, "min_observations": args.min_observations},
                      "source_counts": {"candles": len(rows)}, "observations": observations,
                      "summary": summary, "coverage": detail, "evidence": evidence,
                      "upstream_errors": payload.get("errors", []),
                      "limitations": [
                          "each candle's volume is assigned to one typical-price bin, not true tick-level volume-at-price",
                          "low-volume-node and value-area thresholds are sensitivity parameters",
                          "forward close-to-close movement is descriptive and not an execution or liquidity claim",
                          "missing bars, fees, funding, queue, slippage and fills remain explicit gaps",
                          "no order, wallet, allocation or execution path is included",
                      ], "execution": "research_only_no_orders"},
                     ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
