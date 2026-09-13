#!/usr/bin/env python3
"""Read-only liquidity-stress observer for crypto perpetuals.

The monitor combines executable order-book impact, quoted spread and a
short-horizon EWMA return-volatility estimate.  It is a risk-context example,
not an entry, routing or position-sizing strategy: it never places orders and
never treats unavailable depth as zero depth.
"""

import argparse
import json
import math
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def numeric_levels(levels):
    result = []
    for level in levels or []:
        price = level.get("price")
        qty = level.get("qty")
        if (isinstance(price, (int, float)) and not isinstance(price, bool)
                and isinstance(qty, (int, float)) and not isinstance(qty, bool)
                and math.isfinite(price) and math.isfinite(qty)
                and price > 0 and qty > 0):
            result.append((float(price), float(qty)))
    return result


def depth_notional(levels, top_levels):
    return sum(price * qty for price, qty in numeric_levels(levels)[:top_levels])


def impact_bps(levels, target_notional):
    """Return VWAP impact in bps, or None if target notional is not fillable."""
    valid = numeric_levels(levels)
    if not valid or target_notional <= 0:
        return None
    reference = valid[0][0]
    remaining = float(target_notional)
    base_quantity = 0.0
    for price, qty in valid:
        available = price * qty
        take = min(remaining, available)
        base_quantity += take / price
        remaining -= take
        if remaining <= 1e-9:
            vwap = target_notional / base_quantity
            return abs(vwap / reference - 1.0) * 10_000
    return None


def ewma_vol_bps(closes, alpha):
    """Return per-bar EWMA volatility in bps; no annualization is implied."""
    values = [float(value) for value in closes if isinstance(value, (int, float))
              and not isinstance(value, bool) and math.isfinite(value) and value > 0]
    if len(values) < 2 or not 0 < alpha <= 1:
        return None
    variance = 0.0
    mean = math.log(values[1] / values[0])
    for previous, current in zip(values, values[1:]):
        change = math.log(current / previous)
        mean = alpha * change + (1 - alpha) * mean
        variance = alpha * (change - mean) ** 2 + (1 - alpha) * variance
    return math.sqrt(max(variance, 0.0)) * 10_000


def book_metrics(book, target_notional, top_levels):
    if not book:
        return None
    bids = numeric_levels(book.get("bids", []))
    asks = numeric_levels(book.get("asks", []))
    if not bids or not asks:
        return {
            "best_bid": bids[0][0] if bids else None,
            "best_ask": asks[0][0] if asks else None,
            "spread_bps": None,
            "bid_depth_notional": depth_notional(book.get("bids", []), top_levels),
            "ask_depth_notional": depth_notional(book.get("asks", []), top_levels),
            "sell_impact_bps": None,
            "buy_impact_bps": None,
            "target_notional": target_notional,
            "top_levels": top_levels,
        }
    best_bid, best_ask = bids[0][0], asks[0][0]
    midpoint = (best_bid + best_ask) / 2
    return {
        "best_bid": best_bid,
        "best_ask": best_ask,
        "spread_bps": (best_ask - best_bid) / midpoint * 10_000 if midpoint > 0 else None,
        "bid_depth_notional": depth_notional(book.get("bids", []), top_levels),
        "ask_depth_notional": depth_notional(book.get("asks", []), top_levels),
        "sell_impact_bps": impact_bps(book.get("bids", []), target_notional),
        "buy_impact_bps": impact_bps(book.get("asks", []), target_notional),
        "target_notional": target_notional,
        "top_levels": top_levels,
    }


def classify_stress(metrics, volatility_bps, min_impact_bps, max_spread_bps,
                    min_volatility_bps):
    if metrics is None:
        return "observe_only_missing_order_book"
    impact = [metrics.get("sell_impact_bps"), metrics.get("buy_impact_bps")]
    components = {
        "impact": max(impact) >= min_impact_bps if any(value is not None for value in impact) else None,
        "spread": (metrics["spread_bps"] >= max_spread_bps
                   if metrics.get("spread_bps") is not None else None),
        "volatility": (volatility_bps >= min_volatility_bps
                        if volatility_bps is not None else None),
    }
    available = [value for value in components.values() if value is not None]
    if len(available) < 2:
        return "observe_only_missing_stress_inputs"
    stressed = sum(value is True for value in available)
    if stressed >= 2:
        return "liquidity_stress"
    if stressed == 1:
        return "liquidity_watch"
    return "normal_liquidity"


def observe(base_url, symbol, exchange, target_notional, top_levels, volatility_bars,
            ewma_alpha, min_impact_bps, max_spread_bps, min_volatility_bps, timeout):
    books_payload = fetch(base_url, "/v1/market/order-books", {
        "market": "perp", "symbols": symbol, "exchanges": exchange,
    }, timeout)
    candles_payload = fetch(base_url, "/v1/history/candles", {
        "exchange": exchange, "market": "perp", "symbol": symbol,
        "interval": "1m", "limit": volatility_bars + 1,
    }, timeout)
    book = next((row for row in books_payload.get("books", [])
                 if str(row.get("symbol", "")).upper() == symbol.upper()
                 and str(row.get("exchange", "")).lower() == exchange.lower()), None)
    metrics = book_metrics(book, target_notional, top_levels)
    candles = candles_payload.get("candles", candles_payload.get("klines", []))
    closes = [row.get("close") for row in candles[-(volatility_bars + 1):]]
    volatility = ewma_vol_bps(closes, ewma_alpha)
    state = classify_stress(metrics, volatility, min_impact_bps, max_spread_bps,
                             min_volatility_bps)
    evidence = [
        "order_book_snapshot_available" if book else "missing_order_book_snapshot",
        "volatility_window_available" if volatility is not None else "missing_volatility_window",
    ]
    if metrics and metrics.get("spread_bps") is not None:
        evidence.append(f"spread={metrics['spread_bps']:.2f} bps")
    if metrics and max(metrics.get("sell_impact_bps") or 0,
                       metrics.get("buy_impact_bps") or 0) > 0:
        evidence.append("target-size book impact measured")
    if volatility is not None:
        evidence.append(f"EWMA volatility={volatility:.2f} bps/bar")
    return {
        "symbol": symbol,
        "exchange": exchange,
        "state": state,
        "book": {"ts_ms": book.get("ts_ms") if book else None, "metrics": metrics},
        "ewma_volatility_bps_per_bar": volatility,
        "thresholds": {
            "target_notional": target_notional,
            "min_impact_bps": min_impact_bps,
            "max_spread_bps": max_spread_bps,
            "min_volatility_bps": min_volatility_bps,
            "ewma_alpha": ewma_alpha,
        },
        "evidence": evidence,
        "upstream_errors": books_payload.get("errors", []) + candles_payload.get("errors", []),
        "limitations": [
            "book impact is a snapshot and assumes every displayed level is executable",
            "EWMA volatility is per-bar and not annualized",
            "no fees, latency, queue position, liquidation or position-sizing model",
        ],
        "execution": "research_only_no_orders",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--exchange", default="binance")
    parser.add_argument("--target-notional", type=float, default=10_000.0)
    parser.add_argument("--top-levels", type=int, default=10)
    parser.add_argument("--volatility-bars", type=int, default=60)
    parser.add_argument("--ewma-alpha", type=float, default=0.2)
    parser.add_argument("--min-impact-bps", type=float, default=5.0)
    parser.add_argument("--max-spread-bps", type=float, default=2.0)
    parser.add_argument("--min-volatility-bps", type=float, default=25.0)
    parser.add_argument("--iterations", type=int, default=1)
    parser.add_argument("--interval-secs", type=float, default=30.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (options.target_notional <= 0 or options.top_levels <= 0
            or options.volatility_bars <= 1 or not 0 < options.ewma_alpha <= 1
            or min(options.min_impact_bps, options.max_spread_bps,
                   options.min_volatility_bps) < 0
            or options.iterations <= 0 or options.interval_secs < 0):
        parser.error("invalid target, window, threshold, iteration or interval argument")
    for iteration in range(options.iterations):
        result = observe(
            options.base_url, options.symbol, options.exchange, options.target_notional,
            options.top_levels, options.volatility_bars, options.ewma_alpha,
            options.min_impact_bps, options.max_spread_bps,
            options.min_volatility_bps, options.timeout,
        )
        print(json.dumps({"strategy": "crypto_liquidity_stress_monitor",
                          "iteration": iteration + 1, **result},
                         ensure_ascii=False, sort_keys=True))
        if iteration + 1 < options.iterations:
            time.sleep(options.interval_secs)


if __name__ == "__main__":
    main()
