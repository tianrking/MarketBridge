#!/usr/bin/env python3
"""Deterministic tests for cross-venue price-gap replay."""

import unittest

from crypto_cross_venue_price_gap_replay import aligned_points, gap_observations, summarize


class CrossVenuePriceGapTests(unittest.TestCase):
    def test_alignment_requires_shared_candle_timestamps(self):
        self.assertEqual(aligned_points([(1, 100.0), (2, 101.0)], [(2, 100.0), (3, 99.0)]),
                         [(2, 101.0, 100.0)])

    def test_extreme_gap_that_contracts_is_counted(self):
        points = [(index, 100.0 * gap, 100.0) for index, gap in enumerate(
            [1.0, 1.001, 0.999, 1.0, 1.0, 1.2, 1.05, 1.02]
        )]
        signals = gap_observations(points, 4, 2, 2.0)
        self.assertEqual(len(signals), 1)
        self.assertTrue(signals[0]["contracted"])

    def test_empty_summary_is_observe_only(self):
        self.assertEqual(summarize([], 2, 0.0, 0.0)["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
