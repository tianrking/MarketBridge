#!/usr/bin/env python3
import argparse
import json
import os
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

RETRYABLE_HTTP_CODES = {408, 429, 500, 502, 503, 504}


def parse_args():
    parser = argparse.ArgumentParser(
        description=(
            "Find perpetual contracts whose funding_rate_pct is inside a target "
            "range by calling MarketBridge's raw perpetual funding endpoint."
        )
    )
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--exchange", help="Single exchange, for example binance or bybit.")
    parser.add_argument("--exchanges", help="Comma-separated exchanges, for example binance,okx,bybit.")
    parser.add_argument("--quote", default="USDT")
    parser.add_argument("--symbols", help="Comma-separated symbols, for example BTCUSDT,ETHUSDT.")
    parser.add_argument("--min-pct", type=float, default=-2.0, help="Lower bound in percent.")
    parser.add_argument("--max-pct", type=float, default=-0.1, help="Upper bound in percent.")
    parser.add_argument("--limit", type=int, default=50000)
    parser.add_argument("--timeout", type=float, default=30.0, help="HTTP timeout seconds.")
    parser.add_argument("--retries", type=int, default=3, help="Attempts for transient HTTP/network errors.")
    parser.add_argument("--include-inactive", action="store_true", help="Do not filter inactive contracts.")
    parser.add_argument("--skip-health-check", action="store_true")
    parser.add_argument("--fail-on-exchange-errors", action="store_true")
    parser.add_argument("--json", action="store_true", help="Print raw JSON rows.")
    return parser.parse_args()


def fetch_json(base_url, path, params, timeout, retries):
    query = f"?{urlencode(params)}" if params else ""
    url = f"{base_url.rstrip('/')}{path}{query}"
    headers = {}
    api_key = os.getenv("MARKETBRIDGE_API_KEY")
    if api_key:
        headers["x-api-key"] = api_key
    request = Request(url, headers=headers)

    attempts = max(1, retries)
    last_error = None
    for attempt in range(1, attempts + 1):
        try:
            with urlopen(request, timeout=timeout) as response:
                return json.load(response)
        except HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            last_error = f"HTTP {error.code}: {body}"
            if error.code not in RETRYABLE_HTTP_CODES or attempt == attempts:
                raise SystemExit(last_error) from error
        except URLError as error:
            last_error = f"Cannot connect to MarketBridge: {error}"
            if attempt == attempts:
                raise SystemExit(last_error) from error
        time.sleep(min(2 ** (attempt - 1), 8))
    raise SystemExit(last_error or "MarketBridge request failed")


def require_healthy(args):
    payload = fetch_json(
        args.base_url,
        "/health",
        None,
        timeout=args.timeout,
        retries=args.retries,
    )
    if payload.get("ok") is not True:
        raise SystemExit(f"MarketBridge health check failed: {payload}")


def main():
    args = parse_args()
    if bool(args.exchange) == bool(args.exchanges):
        raise SystemExit("Specify exactly one of --exchange or --exchanges")
    if args.min_pct > args.max_pct:
        raise SystemExit("--min-pct must be <= --max-pct")
    if args.limit <= 0:
        raise SystemExit("--limit must be positive")
    if not args.skip_health_check:
        require_healthy(args)

    params = {
        "quote": args.quote,
        "active_only": "false" if args.include_inactive else "true",
        "limit": str(args.limit),
    }
    if args.exchange:
        params["exchange"] = args.exchange
    if args.exchanges:
        params["exchanges"] = args.exchanges
    if args.symbols:
        params["symbols"] = args.symbols

    payload = fetch_json(
        args.base_url,
        "/v1/market/perpetual-funding",
        params,
        timeout=args.timeout,
        retries=args.retries,
    )
    rows = payload.get("funding", [])
    errors = payload.get("errors", [])
    if not isinstance(rows, list):
        raise SystemExit(f"Unexpected MarketBridge response: funding is not a list: {payload}")
    if errors and args.fail_on_exchange_errors:
        raise SystemExit(f"MarketBridge exchange errors: {json.dumps(errors, ensure_ascii=False)}")

    matches = [
        row
        for row in rows
        if isinstance(row, dict)
        if isinstance(row.get("funding_rate_pct"), (int, float))
        and args.min_pct <= row["funding_rate_pct"] <= args.max_pct
    ]
    matches.sort(key=lambda row: row["funding_rate_pct"])

    if args.json:
        print(
            json.dumps(
                {
                    "query": params,
                    "rows_scanned": len(rows),
                    "count": len(matches),
                    "errors": errors,
                    "funding": matches,
                },
                indent=2,
            )
        )
        return

    print(
        f"Extreme negative funding: {len(matches)} matches "
        f"from {len(rows)} rows ({args.min_pct:.4f}% to {args.max_pct:.4f}%)"
    )
    if errors:
        print(f"Exchange errors: {json.dumps(errors, ensure_ascii=False)}", file=sys.stderr)
    if not matches:
        return

    print(
        f"{'exchange':<10} {'symbol':<18} {'funding_pct':>12} "
        f"{'mark':>14} {'next_funding_ms':>16}"
    )
    for row in matches:
        mark = row.get("mark_price")
        mark_text = "" if mark is None else f"{mark:.8g}"
        print(
            f"{row.get('exchange', ''):<10} "
            f"{row.get('symbol', ''):<18} "
            f"{row['funding_rate_pct']:>11.6f}% "
            f"{mark_text:>14} "
            f"{str(row.get('next_funding_time_ms') or ''):>16}"
        )


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(0)
