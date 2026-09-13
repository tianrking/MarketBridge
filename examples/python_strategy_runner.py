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
    return {
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--strategy", choices=("squeeze", "exhaustion", "basis", "liquidation"), required=True)
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.interval_secs < 0 or args.iterations <= 0:
        parser.error("interval must be non-negative and iterations must be positive")

    strategies = {
        "squeeze": score_squeeze,
        "exhaustion": score_exhaustion,
        "basis": score_basis,
        "liquidation": score_liquidation,
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
