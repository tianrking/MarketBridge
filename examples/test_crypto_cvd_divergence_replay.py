#!/usr/bin/env python3
"""Deterministic tests for CVD divergence replay."""

import unittest

from crypto_cvd_divergence_replay import divergence_observations, flow_window, summarize, trade_rows


class CvdDivergenceTests(unittest.TestCase):
    def test_trade_rows_keep_only_signed_positive_notional(self):
        rows = trade_rows({"rows": [
            {"ts_ms": 1, "notional": 10, "side": "buy"},
            {"ts_ms": 2, "notional": 0, "side": "sell"},
            {"ts_ms": 3, "notional": 5, "side": "unknown"},
        ]})
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["side"], "buy")

    def test_bearish_divergence_that_reverses_is_counted(self):
        candles = [(1, 100.0), (2, 100.0), (3, 101.0), (4, 102.0), (5, 100.0), (6, 99.0)]
        trades = [
            {"ts_ms": 1, "notional": 100.0, "side": "buy"},
            {"ts_ms": 2, "notional": 100.0, "side": "sell"},
            {"ts_ms": 3, "notional": 500.0, "side": "sell"},
        ]
        signals = divergence_observations(candles, trades, 3, 1, 0.5, 0.2)
        self.assertTrue(signals)
        self.assertEqual(signals[0]["divergence"], "bearish")
        self.assertTrue(signals[0]["reversed"])

    def test_flow_window_preserves_missing_as_none(self):
        self.assertEqual(flow_window([], 1, 2)["ratio"], None)
        self.assertEqual(summarize([], 2, 0.0, 0.0)["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
