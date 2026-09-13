#!/usr/bin/env python3
"""Deterministic tests for cross-asset momentum replay."""

import unittest

from crypto_cross_asset_momentum_replay import (
    aligned_points,
    candle_points,
    evaluate_momentum,
    momentum_observation,
)


class CrossAssetMomentumTests(unittest.TestCase):
    def setUp(self):
        self.series = {
            "BTCUSDT": [(index, value) for index, value in enumerate(
                [100.0, 101.0, 102.0, 104.0, 106.0, 108.0, 110.0])],
            "ETHUSDT": [(index, 100.0) for index in range(7)],
            "SOLUSDT": [(index, value) for index, value in enumerate(
                [100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 94.0])],
        }

    def test_candle_points_deduplicates_and_rejects_invalid_closes(self):
        points = candle_points({"candles": [
            {"open_time_ms": 2, "close": 102.0},
            {"open_time_ms": 1, "close": 101.0},
            {"open_time_ms": 2, "close": 103.0},
            {"open_time_ms": 3, "close": 0.0},
        ]})
        self.assertEqual(points, [(1, 101.0), (2, 103.0)])

    def test_alignment_uses_only_shared_timestamps(self):
        aligned = aligned_points({
            "BTC": [(1, 100.0), (2, 101.0)],
            "ETH": [(2, 200.0), (3, 201.0)],
        })
        self.assertEqual(aligned, [(2, {"BTC": 101.0, "ETH": 200.0})])

    def test_top_momentum_beats_equal_weight_benchmark_in_fixture(self):
        aligned = aligned_points(self.series)
        observation = momentum_observation(aligned, 2, 2, 1, 1)
        self.assertEqual(observation["selected_symbols"], ["BTCUSDT"])
        self.assertGreater(observation["edge_bps"], 0.0)
        result = evaluate_momentum(self.series, 2, 1, 1, 0.0, 3)
        self.assertEqual(result["verdict"], "momentum_candidate")
        self.assertEqual(result["observations"], 4)
        self.assertGreater(result["positive_edge_hit_rate"], 0.5)

    def test_missing_window_stays_observe_only(self):
        result = evaluate_momentum({"BTC": [(0, 100.0), (1, 101.0)]}, 2, 1, 1, 0.0, 1)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertEqual(result["observations"], 0)
        self.assertIn("missing_cross_asset_window", result["evidence"])


if __name__ == "__main__":
    unittest.main()
