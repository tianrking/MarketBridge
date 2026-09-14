#!/usr/bin/env python3
"""Replay a paper spot/perpetual funding-carry ledger.

For each fixed funding-event window this case computes, per unit notional, the
funding transfer and the change in the observed spot/perpetual basis.  A
``short_perp`` window receives positive funding and benefits when basis narrows;
``long_perp`` applies the opposite signs.  The calculation is a transparent
accounting decomposition, not a fill, margin, borrow, or live carry trade.
"""

import argparse
import bisect
import json
import statistics
import time
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def fetch(base_url, path, params, timeout):
    query = urlencode({key: value for key, value in params.items() if value is not None})
    request = Request(f"{base_url.rstrip('/')}{path}?{query}")
    with urlopen(request, timeout=timeout) as response:
        return json.load(response)


def number(value):
    if isinstance(value, bool):
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def funding_points(payload):
    intervals = {
        point.get("funding_time_ms"): point.get("interval_ms")
        for point in payload.get("funding_schedule", {}).get("points", [])
        if isinstance(point, dict)
        and isinstance(point.get("funding_time_ms"), int)
        and isinstance(point.get("interval_ms"), int)
        and point["interval_ms"] > 0
    }
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        rate = number(row.get("close"))
        if isinstance(timestamp, int) and rate is not None:
            points.append((timestamp, rate, intervals.get(timestamp)))
    return sorted(set(points))


def price_points(payload):
    points = []
    for row in payload.get("candles", []):
        timestamp = row.get("open_time_ms")
        close = number(row.get("close"))
        if isinstance(timestamp, int) and close is not None and close > 0:
            points.append((timestamp, close))
    return sorted(set(points))


def latest_price(timestamp, points):
    index = bisect.bisect_right([point[0] for point in points], timestamp) - 1
    return points[index][1] if index >= 0 else None


def paper_window(funding, spot, perp, start_index, horizon_events, position_side):
    end_index = start_index + horizon_events
    if end_index >= len(funding):
        return None
    start_ts = funding[start_index][0]
    end_ts = funding[end_index][0]
    spot_start = latest_price(start_ts, spot)
    perp_start = latest_price(start_ts, perp)
    spot_end = latest_price(end_ts, spot)
    perp_end = latest_price(end_ts, perp)
    if any(value is None or value <= 0
           for value in (spot_start, perp_start, spot_end, perp_end)):
        return None
    side = 1.0 if position_side == "short_perp" else -1.0
    funding_sum = sum(row[1] for row in funding[start_index:end_index])
    basis_start_pct = (perp_start / spot_start - 1.0) * 100.0
    basis_end_pct = (perp_end / spot_end - 1.0) * 100.0
    gross_funding_pct = side * funding_sum * 100.0
    basis_pnl_pct = side * (basis_start_pct - basis_end_pct)
    return {
        "start_ts_ms": start_ts,
        "end_ts_ms": end_ts,
        "funding_events": horizon_events,
        "funding_rate_sum": funding_sum,
        "gross_funding_pct": gross_funding_pct,
        "basis_start_pct": basis_start_pct,
        "basis_end_pct": basis_end_pct,
        "basis_pnl_pct": basis_pnl_pct,
        "paper_carry_pct": gross_funding_pct + basis_pnl_pct,
        "funding_interval_ms": funding[start_index][2],
    }


def stats(rows):
    values = [row["paper_carry_pct"] for row in rows]
    funding = [row["gross_funding_pct"] for row in rows]
    basis = [row["basis_pnl_pct"] for row in rows]
    return {
        "windows": len(rows),
        "mean_paper_carry_pct": statistics.mean(values) if values else None,
        "median_paper_carry_pct": statistics.median(values) if values else None,
        "positive_paper_carry_fraction": (sum(value > 0 for value in values) / len(values)
                                           if values else None),
        "mean_gross_funding_pct": statistics.mean(funding) if funding else None,
        "mean_basis_pnl_pct": statistics.mean(basis) if basis else None,
    }


def summarize(rows, min_observations):
    by_state = {}
    for state in ("positive_funding", "negative_funding", "mixed_funding"):
        selected = [row for row in rows if row["funding_state"] == state]
        by_state[state] = stats(selected)
    return {
        "windows": len(rows),
        "by_funding_state": by_state,
        "verdict": ("paper_carry_decomposition_reported"
                     if len(rows) >= min_observations
                     else "observe_only_insufficient_carry_windows"),
        "research_only": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--symbol", default="BTCUSDT")
    parser.add_argument("--funding-exchange", default="binance")
    parser.add_argument("--spot-exchange", default="binance")
    parser.add_argument("--perp-exchange", default="binance")
    parser.add_argument("--price-interval", default="5m")
    parser.add_argument("--days", type=float, default=7.0)
    parser.add_argument("--funding-limit", type=int, default=500)
    parser.add_argument("--price-limit", type=int, default=1000)
    parser.add_argument("--horizon-events", type=int, default=3)
    parser.add_argument("--position-side", choices=("short_perp", "long_perp"), default="short_perp")
    parser.add_argument("--min-observations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    if (args.days <= 0 or args.funding_limit <= 0 or args.price_limit <= 0
            or args.horizon_events <= 0 or args.min_observations <= 0 or args.timeout <= 0):
        parser.error("days, limits, horizon-events and observations must be positive")
    end_ms = int(time.time() * 1000)
    start_ms = end_ms - int(args.days * 86_400_000)
    common = {"symbol": args.symbol, "start_ms": start_ms, "end_ms": end_ms}
    funding_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.funding_exchange, "candle_type": "funding_rate",
        "limit": min(args.funding_limit, 500),
    }, args.timeout)
    spot_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.spot_exchange, "candle_type": "spot",
        "interval": args.price_interval, "limit": min(args.price_limit, 1000),
    }, args.timeout)
    perp_payload = fetch(args.base_url, "/v1/history/candles", {
        **common, "exchange": args.perp_exchange, "candle_type": "perp",
        "interval": args.price_interval, "limit": min(args.price_limit, 1000),
    }, args.timeout)
    funding = funding_points(funding_payload)
    spot = price_points(spot_payload)
    perp = price_points(perp_payload)
    rows = []
    for start_index in range(len(funding)):
        row = paper_window(funding, spot, perp, start_index, args.horizon_events,
                           args.position_side)
        if row is None:
            continue
        if row["gross_funding_pct"] > 0:
            state = "positive_funding"
        elif row["gross_funding_pct"] < 0:
            state = "negative_funding"
        else:
            state = "mixed_funding"
        rows.append({**row, "funding_state": state})
    errors = [{"source": source, "error": payload["error"]}
              for source, payload in (("funding", funding_payload), ("spot", spot_payload),
                                      ("perp", perp_payload))
              if payload.get("error")]
    print(json.dumps({
        "strategy": "crypto_funding_carry_accrual_replay",
        "symbol": args.symbol,
        "venues": {"funding": args.funding_exchange, "spot": args.spot_exchange,
                    "perp": args.perp_exchange},
        "position_side": args.position_side,
        "window": {"start_ms": start_ms, "end_ms": end_ms, "days": args.days},
        "filters": {"price_interval": args.price_interval,
                    "horizon_events": args.horizon_events,
                    "min_observations": args.min_observations},
        "source_counts": {"funding_points": len(funding), "spot_bars": len(spot),
                           "perp_bars": len(perp)},
        "observations": rows,
        "summary": summarize(rows, args.min_observations),
        "coverage": {"funding": funding_payload.get("coverage_detail"),
                      "spot": spot_payload.get("coverage_detail"),
                      "perp": perp_payload.get("coverage_detail")},
        "upstream_errors": errors,
        "limitations": [
            "funding transfer is normalized per unit notional and is not an account cash ledger",
            "basis PnL uses public candle closes, not mark prices or executable bid/ask legs",
            "borrow, margin, collateral, fees, slippage, funding timestamp mismatch and fills are excluded",
            "paper_carry_pct is an accounting decomposition, not a profitability or execution claim",
        ],
        "execution": "research_only_no_orders",
    }, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
