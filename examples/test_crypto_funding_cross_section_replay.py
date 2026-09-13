#!/usr/bin/env python3
"""Deterministic tests for cross-sectional funding replay."""

import unittest

from crypto_funding_cross_section_replay import (
    aligned_price_points,
    cross_section_observations,
    summarize,
)


class FundingCrossSectionTests(unittest.TestCase):
    def test_price_alignment_requires_exact_timestamp_intersection(self):
        aligned = aligned_price_points({
            "BTCUSDT": [(1, 100.0), (2, 101.0)],
            "ETHUSDT": [(2, 10.0), (3, 11.0)],
        })
        self.assertEqual(aligned, [(2, {"BTCUSDT": 101.0, "ETHUSDT": 10.0})])

    def test_low_funding_minus_high_funding_is_measured(self):
        funding = {
            "BTCUSDT": [(1, -0.001, 10_000)],
            "ETHUSDT": [(1, 0.001, 10_000)],
        }
        prices = {
            "BTCUSDT": [(1, 100.0), (2, 102.0)],
            "ETHUSDT": [(1, 100.0), (2, 99.0)],
        }
        observations = cross_section_observations(funding, prices, 1, 1, 1.0, 1.5)
        self.assertEqual(len(observations), 1)
        self.assertEqual(observations[0]["low_funding_symbols"], ["BTCUSDT"])
        self.assertAlmostEqual(observations[0]["low_minus_high_forward_return_pct"], 3.0)

    def test_summary_keeps_insufficient_history_observe_only(self):
        result = summarize([], min_observations=2, min_edge_bps=0.0, paper_cost_bps=0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertEqual(result["observations"], 0)


if __name__ == "__main__":
    unittest.main()
