#!/usr/bin/env python3
"""Deterministic tests for stablecoin depeg research."""

import unittest

from crypto_stablecoin_depeg_monitor import split_stable_pair, summarize_quotes
from crypto_stablecoin_depeg_replay import summarize_records


def quote(symbol, mid, exchange="binance"):
    return {"symbol": symbol, "exchange": exchange,
            "payload": {"bid": mid - 0.0001, "ask": mid + 0.0001},
            "freshness": {"ts_source": 1000}}


def record(deviation, price, timestamp):
    return {"recorded_at_ms": timestamp, "observation": {
        "worst_quote": {"abs_deviation_bps": deviation},
        "risk_asset": {"mid": price},
    }}


class StablecoinDepegTests(unittest.TestCase):
    def test_pair_parser_uses_longest_stable_asset_prefix(self):
        self.assertEqual(split_stable_pair("USDTUSDC"), ("USDT", "USDC"))
        self.assertEqual(split_stable_pair("DAI-USDT"), ("DAI", "USDT"))
        self.assertIsNone(split_stable_pair("BTCUSDT"))

    def test_summary_marks_depeg_stress(self):
        result = summarize_quotes([quote("USDTUSDC", 0.99), quote("BTCUSDT", 60_000)],
                                  "BTCUSDT", "binance", 20, 50, 40)
        self.assertEqual(result["state"], "stablecoin_depeg_stress")
        self.assertEqual(result["risk_asset"]["mid"], 60_000)

    def test_summary_keeps_missing_quotes_visible(self):
        result = summarize_quotes([], "BTCUSDT", "binance", 20, 50, 40)
        self.assertEqual(result["state"], "observe_only_missing_stablecoin_quotes")

    def test_replay_compares_stress_and_ordinary_absolute_moves(self):
        result = summarize_records([
            record(60, 100, 1), record(10, 101, 2), record(10, 102, 3), record(10, 103, 4),
        ], horizon_snapshots=1, stress_bps=50, min_stress=1, min_ordinary=1)
        self.assertEqual(result["stress_observations"], 1)
        self.assertEqual(result["verdict"], "stablecoin_depeg_contagion_candidate")


if __name__ == "__main__":
    unittest.main()
