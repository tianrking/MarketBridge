#!/usr/bin/env python3
"""Export an explicit Binance USDT perpetual universe from MarketBridge."""

import argparse
import json
import re
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch_catalog(base_url, limit, timeout):
    query = urlencode({"exchange": "binance", "quote": "USDT",
                       "active_only": "true", "limit": limit})
    request = Request(f"{base_url.rstrip('/')}/v1/catalog/perpetuals?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def extract_symbols(payload, max_symbols=None):
    symbols = []
    for exchange in payload.get("exchanges", []) if isinstance(payload, dict) else []:
        if str(exchange.get("exchange", "")).lower() != "binance":
            continue
        for contract in exchange.get("contracts", []) or []:
            symbol = str(contract.get("symbol", "")).upper()
            if re.fullmatch(r"[A-Z0-9._-]+USDT", symbol) and symbol not in symbols:
                symbols.append(symbol)
    symbols.sort()
    return symbols[:max_symbols] if max_symbols else symbols


def yaml_snippet(symbols):
    values = ", ".join(symbols)
    return f"symbols: [{values}]\nperp_symbols: [{values}]\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--limit", type=int, default=50000)
    parser.add_argument("--max-symbols", type=int, default=100,
                        help="keep the explicit universe bounded")
    parser.add_argument("--output", default="",
                        help="optional YAML snippet path; stdout is always JSON metadata")
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if args.limit <= 0 or args.max_symbols <= 0 or args.timeout <= 0:
        parser.error("limit, max-symbols and timeout must be positive")
    payload = fetch_catalog(args.base_url, args.limit, args.timeout)
    symbols = extract_symbols(payload, args.max_symbols)
    if not symbols:
        raise SystemExit("MarketBridge returned no active Binance USDT perpetuals")
    snippet = yaml_snippet(symbols)
    if args.output:
        path = Path(args.output)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(snippet, encoding="utf-8")
    print(json.dumps({"exchange": "binance", "quote": "USDT",
                      "symbols": symbols, "count": len(symbols),
                      "yaml_output": args.output or None}, ensure_ascii=False))
    print(snippet, end="")


if __name__ == "__main__":
    main()
