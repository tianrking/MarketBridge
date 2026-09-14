#!/usr/bin/env python3
"""Deterministic tests for the aggregate market-regime response replay."""

import unittest

from crypto_market_regime_replay import summarize_records


def record(price, timestamp, regime_name):
    return {"recorded_at_ms": timestamp, "observation": {
        "price": {"price": price}, "regime": regime_name,
    }}


class MarketRegimeReplayTests(unittest.TestCase):
    def test_regime_bucket_reports_forward_and_absolute_response(self):
        result = summarize_records([
            record(100, 1, "high_volatility"), record(98, 2, "high_volatility"),
            record(95, 3, "high_volatility"),
        ], 2, 1, 0)
        bucket = result["by_regime"]["high_volatility"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], -5.0)
        self.assertAlmostEqual(bucket["mean_absolute_return_pct"], 5.0)

    def test_missing_price_is_not_zero(self):
        result = summarize_records([
            record(100, 1, "normal"),
            {"recorded_at_ms": 2, "observation": {"regime": "normal"}},
            record(102, 3, "normal"),
        ], 1, 1, 0)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_regime_observations")

    def test_regime_states_remain_separate(self):
        result = summarize_records([
            record(100, 1, "fragmented"), record(101, 2, "normal"),
            record(102, 3, "normal"), record(103, 4, "normal"),
        ], 2, 1, 0)
        self.assertIn("fragmented", result["by_regime"])


if __name__ == "__main__":
    unittest.main()
