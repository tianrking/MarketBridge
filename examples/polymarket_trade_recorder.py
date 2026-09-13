#!/usr/bin/env python3
"""Record a bounded, deduplicated Polymarket trade sample as JSONL.

The recorder uses only MarketBridge's public research endpoint. It is a
reproducibility helper, not a wallet tracker or execution component. The
public Data API exposes offset pagination with a bounded range, so the script
refuses unbounded collection and refuses to overwrite an existing file.
"""

import argparse
import json
from pathlib import Path
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


def trade_key(trade):
    transaction_hash = trade.get("transaction_hash")
    if transaction_hash:
        return ("tx", transaction_hash, trade.get("asset"), trade.get("side"))
    return (
        "row",
        trade.get("timestamp"),
        trade.get("asset"),
        trade.get("side"),
        trade.get("size"),
        trade.get("price"),
        trade.get("proxy_wallet"),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--market", help="Polymarket condition ID")
    parser.add_argument("--asset", help="Polymarket token ID")
    parser.add_argument("--page-size", type=int, default=1000)
    parser.add_argument("--pages", type=int, default=1)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if not options.market and not options.asset:
        parser.error("one of --market or --asset is required")
    if not 1 <= options.page_size <= 1000:
        parser.error("--page-size must be between 1 and 1000")
    if not 1 <= options.pages <= 10:
        parser.error("--pages must be between 1 and 10")
    if (options.pages - 1) * options.page_size > 10_000:
        parser.error("page range exceeds the public API offset bound")
    if options.output.exists():
        parser.error(f"refusing to overwrite existing file: {options.output}")
    options.output.parent.mkdir(parents=True, exist_ok=True)

    seen = set()
    trades = []
    pages_read = 0
    for page in range(options.pages):
        payload = fetch(
            options.base_url,
            {
                "market": options.market,
                "asset": options.asset,
                "limit": options.page_size,
                "offset": page * options.page_size,
                "taker_only": False,
            },
            options.timeout,
        )
        rows = payload.get("trades", [])
        pages_read += 1
        for trade in rows:
            key = trade_key(trade)
            if key not in seen:
                seen.add(key)
                trades.append(trade)
        if len(rows) < options.page_size:
            break

    with options.output.open("x", encoding="utf-8") as handle:
        for trade in trades:
            handle.write(json.dumps(trade, ensure_ascii=False, sort_keys=True))
            handle.write("\n")

    print(json.dumps({
        "output": str(options.output),
        "pages_read": pages_read,
        "rows_written": len(trades),
        "deduplicated": True,
        "market": options.market,
        "asset": options.asset,
        "limitations": [
            "public trade history is bounded by upstream pagination and retention",
            "JSONL contains observations, not a complete private fill ledger",
        ],
    }, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
