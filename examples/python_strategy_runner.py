#!/usr/bin/env python3
"""Run MarketBridge research strategies without writing Rust.

Rust owns collection, normalization, cache, history and the HTTP API. This
file is the primary strategy-facing entry point for non-engineers: each
strategy is a small Python function over normalized MarketBridge responses.
It only prints research evidence and never places orders.
"""

import argparse
import json
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from crypto_options_skew_monitor import (
    classify_skew,
    classify_term_structure,
    expiry_timestamp,
    payload_row,
    summarize_expiry,
)
from crypto_options_vrp_monitor import annualized_realized_vol_pct
from crypto_cross_asset_momentum_replay import candle_points, evaluate_momentum
from crypto_volatility_breakout_replay import breakout_features
from funding_convergence_monitor import observe as observe_funding_convergence


def fetch(base_url, path, params, timeout=30.0):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def rows(payload, key):
    return payload.get(key, []) if isinstance(payload.get(key), list) else []


def number(row, key):
    value = row.get(key)
    return float(value) if isinstance(value, (int, float)) else None


def matching(rows_, exchange):
    return [
        row for row in rows_
        if not exchange or str(row.get("exchange", "")).lower() == exchange.lower()
    ]


def latest_value(payload, key, exchange):
    candidates = matching(rows(payload, key), exchange)
    return candidates[-1] if candidates else None


def fetch_core(client, args):
    symbol = args.symbol
    data = {
        "funding": client("/v1/market/funding", {"symbols": symbol}),
        "oi": client("/v1/market/open-interest", {"symbols": symbol}),
        "perp_flow": client("/v1/market/order-flow", {
            "market": "perp", "symbol": symbol, "window_ms": 900_000, "limit": 50
        }),
        "spot_flow": client("/v1/market/order-flow", {
            "market": "spot", "symbol": symbol, "window_ms": 900_000, "limit": 50
        }),
        "liquidations": client("/v1/market/liquidations", {"symbols": symbol}),
        "klines": client("/v1/market/klines", {
            "exchange": args.exchange or "binance", "market": "perp",
            "symbol": symbol, "interval": "5m", "limit": 12
        }),
        "basis": client("/v1/market/basis", {"symbols": symbol}),
    }
    if args.strategy in ("options_skew", "options_vrp"):
        data["options"] = client("/v1/options/chains", {
            "venue": args.options_venue,
            "currency": args.currency,
            "include_stale": "false",
        })
    if args.strategy == "options_vrp":
        data["rv_klines"] = client("/v1/history/candles", {
            "exchange": args.exchange or "binance",
            "symbol": symbol,
            "market": "perp",
            "interval": args.rv_interval,
            "limit": args.rv_bars + 1,
        })
    if args.strategy == "volatility_breakout":
        data["breakout_klines"] = client("/v1/history/candles", {
            "exchange": args.exchange or "binance",
            "symbol": symbol,
            "market": "perp",
            "candle_type": "perp",
            "interval": args.breakout_interval,
            "limit": args.breakout_limit,
        })
    if args.strategy == "funding_convergence":
        data["funding_cross"] = client("/v1/market/perpetual-funding", {
            "symbols": symbol,
            "exchanges": args.funding_exchanges,
            "active_only": "true",
            "limit": 100,
        })
    if args.strategy == "cross_asset_momentum":
        data["cross_asset_klines"] = {
            symbol: client("/v1/history/candles", {
                "exchange": args.exchange or "binance",
                "market": "perp",
                "symbol": symbol,
                "interval": args.cross_asset_interval,
                "limit": args.cross_asset_limit,
            })
            for symbol in args.cross_asset_symbols
        }
    return data


def flow_delta(payload, exchange):
    candidates = matching(rows(payload, "order_flow"), exchange)
    row = max(candidates, key=lambda item: item.get("bucket_start_ms", 0), default=None)
    if not row:
        return None
    return number(row, "cumulative_delta_notional") or number(row, "delta_notional")


def oi_change(payload, exchange, previous):
    values = matching(rows(payload, "open_interest"), exchange)
    row = values[-1] if values else None
    if not row:
        return None
    current = number(row, "open_interest")
    venue = str(row.get("exchange", "unknown"))
    if current is None or venue not in previous or previous[venue] <= 0:
        if current is not None:
            previous[venue] = current
        return None
    change = (current - previous[venue]) / previous[venue] * 100.0
    previous[venue] = current
    return change


def kline_return(payload):
    bars = rows(payload, "klines")
    if not bars:
        return None
    first = number(bars[0], "open")
    last = number(bars[-1], "close")
    return (last - first) / first * 100.0 if first and last is not None else None


def score_squeeze(data, args, previous):
    score = 0
    evidence = []
    funding = matching(rows(data["funding"], "funding"), args.exchange)
    rate = min((number(row, "funding_rate") for row in funding if number(row, "funding_rate") is not None), default=None)
    if rate is not None and rate < 0:
        score += 1
        evidence.append(f"negative funding={rate * 100:.4f}%")
    oi = oi_change(data["oi"], args.exchange, previous)
    if oi is not None and oi > 0:
        score += 1
        evidence.append(f"OI rising={oi:.2f}%")
    spot = flow_delta(data["spot_flow"], args.exchange)
    perp = flow_delta(data["perp_flow"], args.exchange)
    if spot is not None and perp is not None and spot > 0 and perp < 0:
        score += 2
        evidence.append(f"spot/perp CVD divergence={spot:.0f}/{perp:.0f}")
    return score, 4, "squeeze candidate" if score >= 3 else "observe only", evidence


def score_exhaustion(data, args, previous):
    score = 0
    evidence = []
    funding = matching(rows(data["funding"], "funding"), args.exchange)
    rate = max((number(row, "funding_rate") for row in funding if number(row, "funding_rate") is not None), default=None)
    if rate is not None and rate > 0:
        score += 1
        evidence.append(f"positive funding={rate * 100:.4f}%")
    oi = oi_change(data["oi"], args.exchange, previous)
    if oi is not None and oi < 0:
        score += 1
        evidence.append(f"OI falling={oi:.2f}%")
    perp = flow_delta(data["perp_flow"], args.exchange)
    if perp is not None and perp < 0:
        score += 1
        evidence.append(f"perp CVD sell-biased={perp:.0f}")
    ret = kline_return(data["klines"])
    if ret is not None and ret < 0:
        score += 1
        evidence.append(f"recent return={ret:.2f}%")
    return score, 4, "exhaustion candidate" if score >= 3 else "observe only", evidence


def score_basis(data, args, _previous):
    score = 0
    evidence = []
    basis = matching(rows(data["basis"], "basis"), args.exchange)
    row = basis[0] if basis else None
    funding = matching(rows(data["funding"], "funding"), args.exchange)
    frow = funding[0] if funding else None
    basis_bps = number(row, "basis_bps") if row else None
    rate = number(frow, "funding_rate") if frow else None
    interval = number(frow, "funding_interval_ms") if frow else None
    if basis_bps is not None and basis_bps > 0:
        score += 1
        evidence.append(f"basis={basis_bps:.2f} bps")
    if rate is not None and rate > 0:
        score += 1
        evidence.append(f"positive funding={rate * 100:.4f}%")
    if interval:
        daily_bps = rate * 86_400_000 / interval * 10_000 if rate is not None else None
        if daily_bps is not None:
            evidence.append(f"funding proxy={daily_bps:.2f} bps/day")
    else:
        evidence.append("funding interval unknown; annualization withheld")
    return score, 2, "carry candidate" if score == 2 and interval else "observe only", evidence


def score_liquidation(data, args, previous):
    score = 0
    evidence = []
    sells = [
        number(row, "price") * number(row, "qty")
        for row in matching(rows(data["liquidations"], "liquidations"), args.exchange)
        if str(row.get("side", "")).lower() == "sell"
        and number(row, "price") is not None and number(row, "qty") is not None
    ]
    if sum(sells) > 0:
        score += 1
        evidence.append(f"sell liquidation notional={sum(sells):.0f}")
    oi = oi_change(data["oi"], args.exchange, previous)
    if oi is not None and oi < 0:
        score += 1
        evidence.append(f"OI falling={oi:.2f}%")
    perp = flow_delta(data["perp_flow"], args.exchange)
    if perp is not None and perp > 0:
        score += 1
        evidence.append(f"perp CVD positive={perp:.0f}")
    ret = kline_return(data["klines"])
    if ret is not None and ret > 0:
        score += 1
        evidence.append(f"price recovery={ret:.2f}%")
    return score, 4, "flush reversal candidate" if score >= 3 else "observe only", evidence


def option_expiry_summaries(payload, args):
    grouped = {}
    now_ts = time.time()
    for row in rows(payload, "chains"):
        option = payload_row(row)
        expiry = option.get("expiry_time")
        expiry_ts = expiry_timestamp(expiry)
        if expiry_ts is None or expiry_ts <= now_ts:
            continue
        grouped.setdefault(expiry, []).append(row)
    summaries = [summarize_expiry(group, expiry, now_ts, args.atm_band,
                                  args.wing_min, args.wing_max)
                 for expiry, group in grouped.items()]
    return sorted(summaries, key=lambda item: item.get("days_to_expiry") or float("inf"))


def score_options_skew(data, args, _previous):
    summaries = option_expiry_summaries(data["options"], args)
    if not summaries:
        return 0, 2, "observe only", ["missing option chain"]
    target = min(summaries, key=lambda item: abs((item.get("days_to_expiry") or 0) - args.expiry_days))
    target["skew_state"] = classify_skew(target, args.min_skew_iv, args.min_skew_iv)
    near = summaries[0]
    far = summaries[1] if len(summaries) > 1 else None
    term_state = classify_term_structure(
        near.get("atm_iv"), far.get("atm_iv") if far else None, args.min_term_slope_iv,
    )
    evidence = [f"target expiry={target['expiry_time']}"]
    if target.get("put_call_skew_iv") is not None:
        evidence.append(f"put-call skew={target['put_call_skew_iv']:.2f} IV points")
    else:
        evidence.append("comparable wing IV missing")
    evidence.append(f"term structure={term_state}")
    score = int(target["skew_state"] == "downside_protection_demand") + int(term_state != "observe_only_missing_term_points")
    verdict = "options skew observation" if score else "observe only"
    return score, 2, verdict, evidence


def score_options_vrp(data, args, _previous):
    summaries = option_expiry_summaries(data["options"], args)
    candles = rows(data["rv_klines"], "candles")
    closes = [number(row, "close") for row in candles if number(row, "close") is not None and number(row, "close") > 0]
    rv = annualized_realized_vol_pct(closes[-(args.rv_bars + 1):], args.rv_interval)
    if not summaries or rv is None:
        return 0, 1, "observe only", ["missing option ATM IV or realized-volatility window"]
    target = min(summaries, key=lambda item: abs((item.get("days_to_expiry") or 0) - args.expiry_days))
    iv = target.get("atm_iv")
    if iv is None:
        return 0, 1, "observe only", ["target expiry ATM IV missing"]
    vrp = iv - rv
    evidence = [f"ATM IV={iv:.2f}%", f"annualized RV={rv:.2f}%", f"IV-RV={vrp:.2f} points"]
    verdict = "implied volatility premium observation" if vrp >= args.vrp_threshold else "observe only"
    return int(vrp >= args.vrp_threshold), 1, verdict, evidence


def score_volatility_breakout(data, args, _previous):
    bars = []
    for row in rows(data["breakout_klines"], "candles"):
        open_time = row.get("open_time_ms")
        high = number(row, "high")
        low = number(row, "low")
        close = number(row, "close")
        if isinstance(open_time, int) and high is not None and low is not None and close is not None:
            bars.append({
                "ts_ms": open_time,
                "high": high,
                "low": low,
                "close": close,
                "volume": number(row, "volume"),
            })
    if len(bars) <= args.breakout_horizon_bars:
        return 0, 3, "observe only", ["insufficient historical candle window"]
    features = breakout_features(
        bars, len(bars) - 1, args.range_bars, args.compression_window,
        args.baseline_window, args.max_compression_ratio,
        args.breakout_buffer, args.volume_multiplier,
    )
    if features is None:
        return 0, 3, "observe only", ["insufficient warm-up bars for breakout features"]
    evidence = []
    if features["regime"].get("ratio") is not None:
        evidence.append(f"compression ratio={features['regime']['ratio']:.3f}")
    else:
        evidence.append("compression ratio missing")
    if features["direction"]:
        evidence.append(f"range breakout={'up' if features['direction'] > 0 else 'down'}")
    else:
        evidence.append("no range breakout")
    if features["volume_ratio"] is not None:
        evidence.append(f"volume ratio={features['volume_ratio']:.2f}")
    else:
        evidence.append("volume ratio missing")
    score = int(features["compressed"]) + int(features["direction"] != 0) + int(features["volume_confirmed"])
    verdict = "volatility breakout observation" if score >= 2 else "observe only"
    return score, 3, verdict, evidence


def score_funding_convergence(data, args, _previous):
    result = observe_funding_convergence(
        data["funding_cross"], args.symbol, args.min_spread_bps_per_hour,
    )
    evidence = list(result.get("evidence", []))
    spread = result.get("spread") or {}
    if spread.get("spread_bps_per_hour") is not None:
        evidence.append(f"gross spread={spread['spread_bps_per_hour']:.2f} bps/hour")
    candidate = result.get("candidate") is not None
    return int(candidate), 1, "funding differential observation" if candidate else "observe only", evidence


def score_cross_asset_momentum(data, args, _previous):
    series = {
        symbol: candle_points(payload)
        for symbol, payload in data.get("cross_asset_klines", {}).items()
    }
    result = evaluate_momentum(
        series, args.cross_asset_lookback, args.cross_asset_horizon,
        args.cross_asset_top_k, args.cross_asset_min_edge_bps,
        args.cross_asset_min_observations,
    )
    evidence = list(result.get("evidence", []))
    if result.get("mean_edge_bps") is not None:
        evidence.append(f"mean top-basket edge={result['mean_edge_bps']:.2f} bps")
    if result.get("positive_edge_hit_rate") is not None:
        evidence.append(f"positive-edge hit rate={result['positive_edge_hit_rate']:.2%}")
    candidate = result.get("verdict") == "momentum_candidate"
    return int(candidate), 1, "cross-asset momentum observation" if candidate else "observe only", evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--strategy", choices=("squeeze", "exhaustion", "basis", "liquidation", "funding_convergence", "cross_asset_momentum", "options_skew", "options_vrp", "volatility_breakout"), required=True)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--options-venue", default="deribit")
    parser.add_argument("--currency", default="BTC")
    parser.add_argument("--expiry-days", type=float, default=30.0)
    parser.add_argument("--atm-band", type=float, default=0.03)
    parser.add_argument("--wing-min", type=float, default=0.85)
    parser.add_argument("--wing-max", type=float, default=1.15)
    parser.add_argument("--min-skew-iv", type=float, default=3.0)
    parser.add_argument("--min-term-slope-iv", type=float, default=3.0)
    parser.add_argument("--rv-interval", default="1h")
    parser.add_argument("--rv-bars", type=int, default=168)
    parser.add_argument("--vrp-threshold", type=float, default=5.0)
    parser.add_argument("--funding-exchanges", default="binance,okx,bybit")
    parser.add_argument("--min-spread-bps-per-hour", type=float, default=0.5)
    parser.add_argument("--cross-asset-symbols", default="BTCUSDT,ETHUSDT,SOLUSDT")
    parser.add_argument("--cross-asset-interval", default="1h")
    parser.add_argument("--cross-asset-limit", type=int, default=240)
    parser.add_argument("--cross-asset-lookback", type=int, default=8)
    parser.add_argument("--cross-asset-horizon", type=int, default=8)
    parser.add_argument("--cross-asset-top-k", type=int, default=1)
    parser.add_argument("--cross-asset-min-edge-bps", type=float, default=0.0)
    parser.add_argument("--cross-asset-min-observations", type=int, default=5)
    parser.add_argument("--breakout-interval", default="5m")
    parser.add_argument("--breakout-limit", type=int, default=100)
    parser.add_argument("--breakout-horizon-bars", type=int, default=6)
    parser.add_argument("--range-bars", type=int, default=12)
    parser.add_argument("--compression-window", type=int, default=12)
    parser.add_argument("--baseline-window", type=int, default=48)
    parser.add_argument("--max-compression-ratio", type=float, default=0.75)
    parser.add_argument("--breakout-buffer", type=float, default=0.0)
    parser.add_argument("--volume-multiplier", type=float, default=1.2)
    args = parser.parse_args()
    if (args.interval_secs < 0 or args.iterations <= 0 or args.expiry_days <= 0
            or args.rv_bars <= 1 or args.min_skew_iv < 0 or args.min_term_slope_iv < 0
            or args.vrp_threshold < 0 or not 0 < args.atm_band < 0.25
            or not 0 < args.wing_min < 1 or args.wing_max <= 1
            or args.wing_min >= args.wing_max):
        parser.error("interval must be non-negative and iterations must be positive")
    if (args.breakout_limit <= 0 or args.breakout_horizon_bars <= 0
            or args.range_bars <= 0 or args.compression_window <= 1
            or args.baseline_window <= 1 or not 0 < args.max_compression_ratio < 2
            or args.breakout_buffer < 0 or args.volume_multiplier < 0):
        parser.error("invalid volatility breakout windows or thresholds")
    if args.min_spread_bps_per_hour < 0:
        parser.error("min-spread-bps-per-hour cannot be negative")
    args.cross_asset_symbols = [
        item.strip().upper() for item in args.cross_asset_symbols.split(",") if item.strip()
    ]
    if (len(set(args.cross_asset_symbols)) < 2 or args.cross_asset_limit <= 0
            or args.cross_asset_lookback <= 0 or args.cross_asset_horizon <= 0
            or args.cross_asset_top_k <= 0
            or args.cross_asset_top_k > len(set(args.cross_asset_symbols))
            or args.cross_asset_min_edge_bps < 0
            or args.cross_asset_min_observations <= 0):
        parser.error("invalid cross-asset symbols, windows, top-k, edge or observation arguments")

    strategies = {
        "squeeze": score_squeeze,
        "exhaustion": score_exhaustion,
        "basis": score_basis,
        "liquidation": score_liquidation,
        "options_skew": score_options_skew,
        "options_vrp": score_options_vrp,
        "volatility_breakout": score_volatility_breakout,
        "funding_convergence": score_funding_convergence,
        "cross_asset_momentum": score_cross_asset_momentum,
    }
    previous = {}

    def client(path, params):
        return fetch(args.base_url, path, params, args.timeout)

    for iteration in range(args.iterations):
        data = fetch_core(client, args)
        score, maximum, verdict, evidence = strategies[args.strategy](data, args, previous)
        print(json.dumps({
            "strategy": args.strategy,
            "symbol": args.symbol,
            "exchange": args.exchange,
            "iteration": iteration + 1,
            "score": score,
            "max_score": maximum,
            "verdict": verdict,
            "evidence": evidence,
            "execution": "research_only_no_orders",
        }, ensure_ascii=False, sort_keys=True))
        if iteration + 1 < args.iterations:
            time.sleep(args.interval_secs)


if __name__ == "__main__":
    main()
