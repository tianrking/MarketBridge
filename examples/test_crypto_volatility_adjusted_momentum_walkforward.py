#!/usr/bin/env python3
"""Deterministic tests for time-ordered momentum holdout replay."""

import unittest

from crypto_volatility_adjusted_momentum_walkforward import evaluate_walkforward


class MomentumWalkforwardTests(unittest.TestCase):
    def test_test_observations_start_at_or_after_split(self):
        series = {
            "BTCUSDT": [(index, 100.0 + index * 1.5) for index in range(30)],
            "ETHUSDT": [(index, 100.0 + index * 0.5) for index in range(30)],
            "SOLUSDT": [(index, 100.0 - index * 0.2) for index in range(30)],
        }
        result = evaluate_walkforward(series, 4, 2, 4, 1, 0.6, 2, 5.0)
        self.assertIn(result["verdict"], ("observe_only", "holdout_edge_survives"))
        split_ts = result["split_ts_ms"]
        self.assertTrue(all(item["ts_ms"] < split_ts for item in result["observations_detail"]
                            if item["split"] == "train"))
        self.assertTrue(all(item["ts_ms"] >= split_ts for item in result["observations_detail"]
                            if item["split"] == "test"))
        self.assertGreater(result["test"]["observations"], 0)

    def test_invalid_split_is_observe_only(self):
        result = evaluate_walkforward({"BTC": [(0, 100.0), (1, 101.0)]}, 2, 1, 2, 1, 0.0, 1)
        self.assertEqual(result["verdict"], "observe_only_invalid_split")


if __name__ == "__main__":
    unittest.main()
