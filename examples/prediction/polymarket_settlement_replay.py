#!/usr/bin/env python3
"""Replay a simple buy-under-price-cap hypothesis on a closed Polymarket.

This is a bounded paper replay over public trades. It estimates settlement PnL
for BUY trades below a price cap and matching the resolved outcome. It does not
claim complete fills, queue priority, fee schedule, latency, or a live strategy.
"""

import argparse
import json
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode(
        {
            key: ("true" if value is True else "false" if value is False else value)
            for key, value in params.items()
            if value is not None
        }
    )
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def load_trades_jsonl(path):
    trades = []
    with Path(path).open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                raise SystemExit(f"invalid JSONL at {path}:{line_number}: {error}") from error
            if not isinstance(value, dict):
                raise SystemExit(f"expected an object at {path}:{line_number}")
            trades.append(value)
    return trades


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--market", required=True, help="Polymarket condition ID")
    parser.add_argument("--max-entry-price", type=float, default=0.80)
    parser.add_argument("--fee-bps", type=float, default=0.0)
    parser.add_argument("--limit", type=int, default=10_000)
    parser.add_argument("--start-ts", type=int, help="include trades at or after Unix seconds")
    parser.add_argument("--end-ts", type=int, help="include trades at or before Unix seconds")
    parser.add_argument("--position-size-usd", type=float, help="fixed paper stake per selected trade")
    parser.add_argument("--initial-capital", type=float, default=1_000.0)
    parser.add_argument("--max-trades", type=int, default=10_000)
    parser.add_argument("--market-page-limit", type=int, default=500)
    parser.add_argument("--market-max-offset", type=int, default=0)
    parser.add_argument("--trades-jsonl", type=Path, help="use a recorded trade sample instead of fetching trades")
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if not 0.0 < options.max_entry_price < 1.0:
        parser.error("--max-entry-price must be between 0 and 1")
    if (
        options.fee_bps < 0
        or options.limit <= 0
        or options.market_page_limit <= 0
        or options.market_max_offset < 0
        or options.initial_capital <= 0
        or options.max_trades <= 0
        or options.position_size_usd is not None
        and options.position_size_usd <= 0
        or options.start_ts is not None
        and options.end_ts is not None
        and options.start_ts > options.end_ts
    ):
        parser.error("invalid capital, time-window, position-size or limit arguments")

    markets = fetch(
        options.base_url,
        "/polymarket/markets",
        {
            "limit": min(options.market_page_limit, 1_000),
            "max_offset": min(options.market_max_offset, 10_000),
            "include_closed": True,
        },
        options.timeout,
    ).get("markets", [])
    market = next(
        (
            row
            for row in markets
            if row.get("market_id") == options.market
            or row.get("condition_id") == options.market
        ),
        None,
    )
    if not market:
        raise SystemExit("market was not found in the closed-market metadata response")
    if market.get("status") != "closed" or not market.get("resolved_outcome"):
        raise SystemExit("market is not closed or has no unambiguous resolved_outcome")

    if options.trades_jsonl:
        trades = load_trades_jsonl(options.trades_jsonl)
        trade_source = "local_jsonl"
    else:
        trades = fetch(
            options.base_url,
            "/v1/prediction/trades",
            {"market": options.market, "limit": min(options.limit, 10_000), "taker_only": False},
            options.timeout,
        ).get("trades", [])
        trade_source = "marketbridge_api"
    selected = [
        trade
        for trade in trades
        if str(trade.get("side") or "").upper() == "BUY"
        and isinstance(trade.get("price"), (int, float))
        and isinstance(trade.get("size"), (int, float))
        and trade["price"] <= options.max_entry_price
        and (
            options.start_ts is None
            or isinstance(trade.get("timestamp"), (int, float))
            and trade["timestamp"] >= options.start_ts
        )
        and (
            options.end_ts is None
            or isinstance(trade.get("timestamp"), (int, float))
            and trade["timestamp"] <= options.end_ts
        )
    ]
    selected.sort(key=lambda trade: trade.get("timestamp", 0))
    selected = selected[: options.max_trades]
    fee_multiplier = 1.0 + options.fee_bps / 10_000.0
    capital = options.initial_capital
    spend = 0.0
    payout = 0.0
    wins = 0
    considered = 0
    skipped_capital = 0
    for trade in selected:
        observed_stake = trade["size"] * trade["price"]
        stake = options.position_size_usd or observed_stake
        total_cost = stake * fee_multiplier
        if total_cost > capital:
            skipped_capital += 1
            continue
        considered += 1
        capital -= total_cost
        spend += total_cost
        if trade.get("outcome") == market["resolved_outcome"]:
            shares = stake / trade["price"]
            payout += shares
            wins += 1
    losses = considered - wins
    capital_after_settlement = capital + payout
    pnl = capital_after_settlement - options.initial_capital
    print(json.dumps({
        "market": options.market,
        "status": market.get("status"),
        "resolved_outcome": market.get("resolved_outcome"),
        "trade_rows_scanned": len(trades),
        "trade_source": trade_source,
        "selected_buy_rows": len(selected),
        "considered_positions": considered,
        "skipped_for_capital": skipped_capital,
        "wins": wins,
        "losses": losses,
        "spend_with_fee": spend,
        "settlement_payout": payout,
        "paper_pnl": pnl,
        "initial_capital": options.initial_capital,
        "capital_after_settlement": capital_after_settlement,
        "paper_return_pct": pnl / spend * 100.0 if spend else None,
        "assumptions": {
            "entry_price_cap": options.max_entry_price,
            "fee_bps": options.fee_bps,
            "one_contract_pays_one_dollar": True,
            "position_size_usd": options.position_size_usd,
            "start_ts": options.start_ts,
            "end_ts": options.end_ts,
        },
        "limitations": [
            "public trades may not be a complete or deduplicated fill ledger",
            "no queue position, latency, slippage, resolution dispute or capital lock time",
            "paper result is not a recommendation or execution result",
        ],
    }, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
