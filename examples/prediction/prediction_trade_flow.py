#!/usr/bin/env python3
"""Summarize public Polymarket trade history through MarketBridge.

This is a research input check for prediction-market replay. It reports side,
notional, VWAP, timestamps and wallet concentration, but does not copy users,
place orders or infer profitability from a short sample.
"""

import argparse
import json
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, params, timeout):
    query = urlencode(
        {
            key: ("true" if value is True else "false" if value is False else value)
            for key, value in params.items()
            if value is not None
        }
    )
    request = Request(f"{base_url.rstrip('/')}/v1/prediction/trades?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--market", help="Polymarket condition ID")
    parser.add_argument("--asset", help="Polymarket token ID")
    parser.add_argument("--limit", type=int, default=1000)
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if not options.market and not options.asset:
        parser.error("one of --market or --asset is required")
    if options.limit <= 0 or options.offset < 0:
        parser.error("limit must be positive and offset cannot be negative")

    payload = fetch(
        options.base_url,
        {
            "market": options.market,
            "asset": options.asset,
            "limit": min(options.limit, 10_000),
            "offset": min(options.offset, 10_000),
            "taker_only": False,
        },
        options.timeout,
    )
    trades = payload.get("trades", [])
    by_side = {}
    wallets = set()
    for trade in trades:
        side = str(trade.get("side") or "UNKNOWN").upper()
        size = trade.get("size")
        price = trade.get("price")
        if not isinstance(size, (int, float)) or not isinstance(price, (int, float)):
            continue
        row = by_side.setdefault(side, {"count": 0, "size": 0.0, "notional": 0.0})
        row["count"] += 1
        row["size"] += size
        row["notional"] += size * price
        if trade.get("proxy_wallet"):
            wallets.add(trade["proxy_wallet"])
    for row in by_side.values():
        row["vwap"] = row["notional"] / row["size"] if row["size"] else None
    timestamps = [
        trade["timestamp"]
        for trade in trades
        if isinstance(trade.get("timestamp"), (int, float))
    ]
    print(json.dumps({
        "rows": len(trades),
        "unique_wallets": len(wallets),
        "first_timestamp": min(timestamps) if timestamps else None,
        "last_timestamp": max(timestamps) if timestamps else None,
        "by_side": by_side,
        "source": payload.get("source"),
        "limitations": [
            "public trade history is an observation set, not a complete fill simulator",
            "fees, queue position, latency, resolution and wallet identity quality require separate validation",
        ],
    }, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
