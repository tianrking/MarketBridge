#!/usr/bin/env python3
"""Compare early, middle and late entries in a resolved Polymarket market.

This is a bounded paper replay of the public observation that 15-minute
markets can have early price discovery and late conviction volume.  It keeps
the claim falsifiable: public BUY observations are bucketed by elapsed time,
then scored against the resolved outcome with a fixed paper stake.  It does
not infer maker status, complete fills, queue position or a trading edge.
"""

import argparse
from collections import defaultdict
from datetime import datetime
import json
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import Request, urlopen


BUCKETS = ("early", "middle", "late")


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
    rows = []
    with Path(path).open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                raise SystemExit(f"invalid JSONL at {path}:{line_number}: {error}") from error
            if isinstance(value, dict):
                rows.append(value)
    return rows


def parse_timestamp(value):
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str) and value:
        try:
            return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
        except ValueError:
            return None
    return None


def trade_key(trade):
    if trade.get("transaction_hash"):
        return ("tx", trade.get("transaction_hash"), trade.get("asset"), trade.get("side"))
    return (
        "row",
        trade.get("timestamp"),
        trade.get("asset"),
        trade.get("side"),
        trade.get("price"),
        trade.get("size"),
        trade.get("proxy_wallet"),
    )


def bucket_name(timestamp, start_ts, end_ts):
    if end_ts <= start_ts:
        return None
    fraction = min(1.0, max(0.0, (timestamp - start_ts) / (end_ts - start_ts)))
    if fraction < 1.0 / 3.0:
        return "early"
    if fraction < 2.0 / 3.0:
        return "middle"
    return "late"


def summarize(rows, resolved_outcome, start_ts, end_ts, stake, fee_bps):
    grouped = defaultdict(list)
    skipped_missing_outcome = 0
    for trade in rows:
        if str(trade.get("side") or "").upper() != "BUY":
            continue
        timestamp = parse_timestamp(trade.get("timestamp"))
        price = trade.get("price")
        size = trade.get("size")
        outcome = trade.get("outcome")
        if timestamp is None or not isinstance(price, (int, float)) or not 0.0 < price <= 1.0:
            continue
        if not isinstance(size, (int, float)) or size <= 0:
            continue
        if not outcome:
            skipped_missing_outcome += 1
            continue
        bucket = bucket_name(timestamp, start_ts, end_ts)
        if bucket:
            grouped[bucket].append((timestamp, float(price), float(size), str(outcome)))

    fee_multiplier = 1.0 + fee_bps / 10_000.0
    result = {}
    for bucket in BUCKETS:
        selected = grouped[bucket]
        spend = 0.0
        payout = 0.0
        wins = 0
        observed_volume = 0.0
        for _, price, size, outcome in selected:
            cost = stake * fee_multiplier
            spend += cost
            observed_volume += size * price
            if outcome == resolved_outcome:
                wins += 1
                payout += stake / price
        pnl = payout - spend
        result[bucket] = {
            "observations": len(selected),
            "wins": wins,
            "losses": len(selected) - wins,
            "win_rate": wins / len(selected) if selected else None,
            "mean_entry_price": sum(row[1] for row in selected) / len(selected) if selected else None,
            "observed_trade_volume": observed_volume,
            "paper_stake_with_fee": spend,
            "settlement_payout": payout,
            "paper_pnl": pnl,
            "paper_return_pct": pnl / spend * 100.0 if spend else None,
        }
    return result, skipped_missing_outcome


def empty_buckets():
    return {
        bucket: {
            "observations": 0,
            "wins": 0,
            "losses": 0,
            "win_rate": None,
            "mean_entry_price": None,
            "observed_trade_volume": 0.0,
            "paper_stake_with_fee": 0.0,
            "settlement_payout": 0.0,
            "paper_pnl": 0.0,
            "paper_return_pct": None,
        }
        for bucket in BUCKETS
    }


def select_market(markets, market_id, market_query):
    query = market_query.casefold() if market_query else None
    for market in markets:
        if market.get("status") != "closed" or not market.get("resolved_outcome"):
            continue
        if market_id and market.get("market_id") != market_id and market.get("condition_id") != market_id:
            continue
        if query and query not in str(market.get("question", "")).casefold():
            continue
        return market
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    selector = parser.add_mutually_exclusive_group(required=True)
    selector.add_argument("--market-id", help="Polymarket condition ID")
    selector.add_argument("--market-query", help="case-insensitive text in the market question")
    parser.add_argument("--market-limit", type=int, default=100)
    parser.add_argument("--max-offset", type=int, default=0)
    parser.add_argument("--limit", type=int, default=10_000)
    parser.add_argument("--trades-jsonl", type=Path)
    parser.add_argument("--position-size-usd", type=float, default=10.0)
    parser.add_argument("--fee-bps", type=float, default=0.0)
    parser.add_argument("--min-bucket-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    options = parser.parse_args()
    if (
        options.market_limit <= 0
        or options.max_offset < 0
        or options.limit <= 0
        or options.position_size_usd <= 0
        or options.fee_bps < 0
        or options.min_bucket_observations <= 0
    ):
        parser.error("limits, stake and fee arguments must be valid positive values")

    markets_payload = fetch(
        options.base_url,
        "/polymarket/markets",
        {
            "limit": min(options.market_limit, 500),
            "max_offset": min(options.max_offset, 10_000),
            "include_closed": True,
            "order": "createdAt",
            "ascending": False,
        },
        options.timeout,
    )
    market = select_market(markets_payload.get("markets", []), options.market_id, options.market_query)
    if not market:
        raise SystemExit("no resolved closed market matched the selector")

    if options.trades_jsonl:
        trades = load_trades_jsonl(options.trades_jsonl)
        trade_source = "local_jsonl"
    else:
        trades = fetch(
            options.base_url,
            "/v1/prediction/trades",
            {
                "market": market.get("condition_id") or market.get("market_id"),
                "limit": min(options.limit, 10_000),
                "taker_only": False,
            },
            options.timeout,
        ).get("trades", [])
        trade_source = "marketbridge_api"

    unique = []
    seen = set()
    for trade in trades:
        key = trade_key(trade)
        if key not in seen:
            seen.add(key)
            unique.append(trade)
    timestamps = [parse_timestamp(trade.get("timestamp")) for trade in unique]
    timestamps = [timestamp for timestamp in timestamps if timestamp is not None]
    if not timestamps:
        print(
            json.dumps(
                {
                    "strategy": "polymarket_timing_replay",
                    "market": {
                        "market_id": market.get("market_id"),
                        "condition_id": market.get("condition_id"),
                        "question": market.get("question"),
                        "resolved_outcome": market.get("resolved_outcome"),
                        "expiry_time": market.get("expiry_time"),
                    },
                    "trade_source": trade_source,
                    "trade_rows_scanned": len(trades),
                    "unique_trade_rows": len(unique),
                    "buckets": empty_buckets(),
                    "summary": {
                        "enough_observations": False,
                        "min_bucket_observations": options.min_bucket_observations,
                        "late_minus_early_return_pct": None,
                        "rows_missing_outcome": 0,
                    },
                    "verdict": "observe only",
                    "missing_context": [
                        "the public trade endpoint returned no timestamped observations for this market",
                        "no timing bucket or paper PnL is inferred from an empty sample",
                    ],
                    "execution": "research_only_no_orders",
                },
                ensure_ascii=False,
                indent=2,
                sort_keys=True,
            )
        )
        return
    start_ts = min(timestamps)
    end_ts = parse_timestamp(market.get("expiry_time")) or max(timestamps)
    if end_ts <= start_ts:
        end_ts = max(timestamps)
    buckets, skipped_missing_outcome = summarize(
        unique,
        market["resolved_outcome"],
        start_ts,
        end_ts,
        options.position_size_usd,
        options.fee_bps,
    )
    enough = all(
        buckets[bucket]["observations"] >= options.min_bucket_observations for bucket in BUCKETS
    )
    early_return = buckets["early"]["paper_return_pct"]
    late_return = buckets["late"]["paper_return_pct"]
    print(
        json.dumps(
            {
                "strategy": "polymarket_timing_replay",
                "hypothesis": "late conviction entries have different resolution-adjusted outcomes than early price-discovery entries",
                "market": {
                    "market_id": market.get("market_id"),
                    "condition_id": market.get("condition_id"),
                    "question": market.get("question"),
                    "resolved_outcome": market.get("resolved_outcome"),
                    "expiry_time": market.get("expiry_time"),
                },
                "timing_anchor": {
                    "start_ts": start_ts,
                    "end_ts": end_ts,
                    "start_definition": "earliest observed public trade, not guaranteed market open",
                    "bucket_definition": "thirds of the observed start-to-end window",
                },
                "trade_source": trade_source,
                "trade_rows_scanned": len(trades),
                "unique_trade_rows": len(unique),
                "buckets": buckets,
                "summary": {
                    "enough_observations": enough,
                    "min_bucket_observations": options.min_bucket_observations,
                    "late_minus_early_return_pct": (
                        late_return - early_return
                        if late_return is not None and early_return is not None
                        else None
                    ),
                    "rows_missing_outcome": skipped_missing_outcome,
                },
                "verdict": "descriptive timing difference" if enough else "observe only",
                "limitations": [
                    "earliest public trade is only a start-time proxy",
                    "public Data API rows are not a complete maker/taker fill ledger",
                    "fixed paper stake ignores capacity, queue position, latency, slippage and capital lock time",
                ],
                "execution": "research_only_no_orders",
            },
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
