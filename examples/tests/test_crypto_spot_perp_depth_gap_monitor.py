#!/usr/bin/env python3
"""Deterministic tests for spot/perp depth-gap monitoring."""

import unittest

from crypto_liquidity_stress_monitor import book_metrics
from crypto_spot_perp_depth_gap_monitor import classify_gap


class SpotPerpDepthGapTests(unittest.TestCase):
    def test_perp_depth_advantage_requires_depth_and_impact_evidence(self):
        spot = book_metrics({
            "bids": [{"price": 100.0, "qty": 5.0}, {"price": 99.0, "qty": 5.0}],
            "asks": [{"price": 100.0, "qty": 5.0}, {"price": 101.0, "qty": 5.0}],
        }, 1_000.0, 1)
        perp = book_metrics({
            "bids": [{"price": 100.0, "qty": 100.0}],
            "asks": [{"price": 100.0, "qty": 100.0}],
        }, 1_000.0, 1)
        self.assertEqual(classify_gap(spot, perp, 2.0, 5.0),
                         "perp_depth_advantage_observation")

    def test_missing_side_stays_explicit(self):
        spot = book_metrics({"bids": [{"price": 100.0, "qty": 10.0}], "asks": []},
                            1_000.0, 1)
        perp = book_metrics({
            "bids": [{"price": 100.0, "qty": 100.0}],
            "asks": [{"price": 100.0, "qty": 100.0}],
        }, 1_000.0, 1)
        self.assertEqual(classify_gap(spot, perp, 2.0, 5.0),
                         "observe_only_missing_target_size_depth")
        self.assertEqual(classify_gap(None, perp, 2.0, 5.0),
                         "observe_only_missing_spot_or_perp_book")


if __name__ == "__main__":
    unittest.main()
