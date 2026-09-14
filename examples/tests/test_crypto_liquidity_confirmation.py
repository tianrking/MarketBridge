"""Deterministic tests for the multi-channel liquidity confirmation case."""

import unittest

from crypto_liquidity_confirmation_monitor import observe
from crypto_liquidity_confirmation_replay import summarize_records


def quote(symbol, exchange, price):
    return {
        "source_ref": {"source": exchange},
        "instrument_ref": {"symbol": symbol, "product_type": "spot"},
        "payload": {"mid": price},
        "freshness": {"ts_source": 1},
    }


def payloads(etf_flow, stable_change, coinbase_price, reference_price, funding_rate):
    return (
        {"signals": [{"source": "farside_etf", "symbol": "BTC",
                       "value": etf_flow, "source_time_ms": 1,
                       "raw": {"date": "2026-09-01"}}]},
        {"data": {"assets": [{"change_7d_pct": stable_change}],
                   "total_supply_usd": 1_000_000}},
        {"quotes": [quote("BTC-USD", "coinbase", coinbase_price),
                    quote("BTCUSDT", "binance", reference_price)]},
        {"funding": [{"exchange": "binance", "symbol": "BTCUSDT",
                       "funding_rate": funding_rate, "funding_interval_ms": 28_800_000}]},
    )


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "state": state,
    }}


class LiquidityConfirmationTests(unittest.TestCase):
    def test_positive_channels_form_risk_on_confirmation(self):
        result = observe(*payloads(150, 2.0, 101.0, 100.0, 0.0001))
        self.assertEqual(result["state"], "risk_on_confirmation")
        self.assertEqual(result["confirmation"]["score"], 3)
        self.assertEqual(result["funding"]["state"], "funding_neutral")

    def test_negative_channels_form_deterioration(self):
        result = observe(*payloads(-150, -2.0, 99.0, 100.0, -0.0001))
        self.assertEqual(result["state"], "liquidity_deterioration")
        self.assertEqual(result["confirmation"]["score"], -3)

    def test_missing_channels_are_not_filled_with_zero(self):
        result = observe({"signals": []}, {"data": {"assets": []}},
                         {"quotes": []}, {"funding": []})
        self.assertEqual(result["state"], "observe_only_insufficient_liquidity_channels")
        self.assertEqual(result["confirmation"]["observed_components"], [])

    def test_replay_keeps_mixed_and_confirmed_buckets_separate(self):
        result = summarize_records([
            record(1, 100, "risk_on_confirmation"),
            record(2, 101, "liquidity_deterioration"),
            record(3, 99, "mixed_liquidity_context"),
            record(4, 102, "risk_on_confirmation"),
        ], 1, 1)
        self.assertEqual(result["by_state"]["risk_on_confirmation"]["observations"], 1)
        self.assertEqual(result["by_state"]["liquidity_deterioration"]["observations"], 1)
        self.assertEqual(result["by_state"]["mixed_liquidity_context"]["observations"], 1)
        self.assertEqual(result["verdict"], "liquidity_confirmation_response_reported")


if __name__ == "__main__":
    unittest.main()
