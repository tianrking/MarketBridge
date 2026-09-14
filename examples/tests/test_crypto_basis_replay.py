#!/usr/bin/env python3
"""Deterministic tests for the basis contraction replay."""

import unittest

from crypto_basis_replay import basis_series, score_series, summarize_series


class BasisReplayTests(unittest.TestCase):
    def test_basis_series_keeps_symbol_and_exchange_identity(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"basis": [
                {"symbol": "BTCUSDT", "exchange": "binance", "basis_bps": 3.0},
                {"symbol": "ETHUSDT", "exchange": "binance", "basis_bps": 99.0},
            ]}},
            {"recorded_at_ms": 2, "observation": {"basis": [
                {"symbol": "BTCUSDT", "exchange": "binance", "basis_bps": 4.0},
            ]}},
        ]
        self.assertEqual(basis_series(records, "BTCUSDT"),
                         {"binance": [(1, 3.0), (2, 4.0)]})

    def test_extreme_basis_that_contracts_is_counted(self):
        points = [(index, value) for index, value in enumerate(
            [1.0, 1.1, 0.9, 1.0, 1.0, 5.0, 3.0, 2.0]
        )]
        signals = score_series(points, lookback=4, horizon=2, min_z=2.0)
        self.assertEqual(len(signals), 1)
        self.assertTrue(signals[0]["contracted"])

    def test_summary_does_not_promote_insufficient_history(self):
        result = summarize_series({"binance": [(0, 1.0), (1, 2.0)]}, 4, 1, 2.0, 1, 0.5)
        self.assertEqual(result["exchanges"]["binance"]["signals"], 0)
        self.assertTrue(result["exchanges"]["binance"]["verdict"].startswith("observe_only"))


if __name__ == "__main__":
    unittest.main()
