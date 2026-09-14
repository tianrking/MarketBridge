#!/usr/bin/env python3
"""Deterministic tests for volatility-adjusted momentum replay."""

import unittest

from crypto_volatility_adjusted_momentum_replay import (
    evaluate_volatility_adjusted_momentum,
    realized_vol_pct,
    volatility_adjusted_observation,
)
from crypto_cross_asset_momentum_replay import aligned_points


class VolatilityAdjustedMomentumTests(unittest.TestCase):
    def setUp(self):
        self.series = {
            "BTCUSDT": [(index, value) for index, value in enumerate(
                [100.0, 101.0, 102.0, 104.0, 106.0, 108.0, 110.0, 112.0, 114.0])],
            "ETHUSDT": [(index, value) for index, value in enumerate(
                [100.0, 100.5, 101.0, 101.5, 102.0, 102.5, 103.0, 103.5, 104.0])],
            "SOLUSDT": [(index, value) for index, value in enumerate(
                [100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 94.0, 93.0, 92.0])],
        }

    def test_realized_volatility_rejects_zero_or_short_series(self):
        self.assertIsNone(realized_vol_pct([100.0]))
        self.assertEqual(realized_vol_pct([100.0, 100.0]), 0.0)
        self.assertGreater(realized_vol_pct([100.0, 101.0, 99.0]), 0.0)

    def test_ranking_contains_volatility_evidence(self):
        aligned = aligned_points(self.series)
        observation = volatility_adjusted_observation(aligned, 5, 2, 1, 3, 1)
        self.assertIsNotNone(observation)
        self.assertIn("BTCUSDT", observation["risk_adjusted_scores"])
        self.assertIn("trailing_volatility_pct_per_bar", observation)

    def test_replay_requires_forward_observations(self):
        result = evaluate_volatility_adjusted_momentum(
            {"BTCUSDT": [(0, 100.0), (1, 101.0)]}, 2, 1, 2, 1, 0.0, 1,
        )
        self.assertEqual(result["verdict"], "observe_only")
        self.assertIn("missing_cross_asset_window_or_volatility", result["evidence"])

    def test_paper_cost_hurdle_reduces_edge_without_claiming_a_fill(self):
        gross = evaluate_volatility_adjusted_momentum(
            self.series, 2, 1, 2, 1, 0.0, 1, False, 0.0,
        )
        costed = evaluate_volatility_adjusted_momentum(
            self.series, 2, 1, 2, 1, 0.0, 1, False, 25.0,
        )
        self.assertAlmostEqual(costed["mean_edge_bps"], gross["mean_edge_bps"])
        self.assertAlmostEqual(
            costed["mean_cost_adjusted_edge_bps"], gross["mean_edge_bps"] - 25.0
        )
        self.assertEqual(costed["paper_cost_bps"], 25.0)


if __name__ == "__main__":
    unittest.main()
