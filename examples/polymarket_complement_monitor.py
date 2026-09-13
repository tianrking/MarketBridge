#!/usr/bin/env python3
"""Research-only Polymarket YES/NO complement-price monitor.

The monitor finds markets where the best ask for YES plus the best ask for NO
is below one. It reports a candidate after a configurable buffer, but never
places or signs an order. Market discovery and book snapshots are still not a
fill simulation: queue position, fees, latency, resolution and capacity are
left explicit.
"""

import argparse
import json
import sys
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode(params)
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--max-markets", type=int, default=50)
    parser.add_argument("--min-edge-bps", type=float, default=10.0)
    parser.add_argument("--min-ask-depth", type=float, default=1.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    return parser.parse_args()


def main():
    options = args()
    if options.limit <= 0 or options.max_markets <= 0:
        raise SystemExit("--limit and --max-markets must be positive")
    if options.min_edge_bps < 0 or options.min_ask_depth < 0:
        raise SystemExit("edge and depth thresholds cannot be negative")

    payload = fetch(
        options.base_url,
        "/polymarket/markets",
        {"limit": options.limit, "max_offset": options.limit},
        options.timeout,
    )
    markets = [
        market
        for market in payload.get("markets", [])
        if market.get("status") == "active"
        and len(market.get("clob_token_ids", [])) >= 2
        and len(market.get("outcomes", [])) >= 2
    ][: options.max_markets]
    token_ids = sorted(
        {
            token_id
            for market in markets
            for token_id in market.get("clob_token_ids", [])[:2]
        }
    )
    books_payload = fetch(
        options.base_url,
        "/polymarket/books",
        {"token_ids": ",".join(token_ids)},
        options.timeout,
    )
    books = {
        book.get("asset_id"): book
        for book in books_payload.get("books", [])
        if book.get("asset_id")
    }
    threshold = 1.0 - options.min_edge_bps / 10_000.0
    candidates = []
    for market in markets:
        yes_id, no_id = market["clob_token_ids"][:2]
        yes, no = books.get(yes_id), books.get(no_id)
        if not yes or not no:
            continue
        yes_ask, no_ask = yes.get("best_ask"), no.get("best_ask")
        yes_depth, no_depth = yes.get("ask_depth"), no.get("ask_depth")
        if not all(isinstance(value, (int, float)) for value in (yes_ask, no_ask, yes_depth, no_depth)):
            continue
        combined = yes_ask + no_ask
        if combined >= threshold or min(yes_depth, no_depth) < options.min_ask_depth:
            continue
        candidates.append(
            {
                "market_id": market.get("market_id"),
                "question": market.get("question"),
                "yes_ask": yes_ask,
                "no_ask": no_ask,
                "combined_ask": combined,
                "gross_edge_bps": (1.0 - combined) * 10_000.0,
                "yes_ask_depth": yes_depth,
                "no_ask_depth": no_depth,
                "expiry_time": market.get("expiry_time"),
            }
        )
    candidates.sort(key=lambda row: row["gross_edge_bps"], reverse=True)

    print(
        f"Polymarket complement candidates: {len(candidates)} "
        f"(scanned={len(markets)}, min_edge_bps={options.min_edge_bps:g})"
    )
    for candidate in candidates:
        print(json.dumps(candidate, ensure_ascii=False, sort_keys=True))
    if not candidates:
        print("No candidate passed the snapshot thresholds; this is not evidence of no market edge.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"MarketBridge request or response failed: {error}", file=sys.stderr)
        raise SystemExit(1)
