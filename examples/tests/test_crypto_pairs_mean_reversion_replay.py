#!/usr/bin/env python3
"""Deterministic tests for crypto pair mean-reversion replay."""

import unittest

from crypto_pairs_mean_reversion_replay import aligned_points, spread_observations, summarize


class PairMeanReversionTests(unittest.TestCase):
    def test_alignment_keeps_only_shared_timestamps(self):
        self.assertEqual(
            aligned_points([(1, 100.0), (2, 101.0)], [(2, 10.0), (3, 11.0)]),
            [(2, 101.0, 10.0)],
        )

    def test_extreme_spread_that_reverts_is_counted(self):
        points = [(index, 100.0 * spread) for index, spread in enumerate(
            [1.0, 1.01, 0.99, 1.0, 1.0, 1.2, 1.05, 1.02]
        )]
        # Equal second leg makes the log spread track the supplied first leg.
        points = [(timestamp, left, 100.0) for timestamp, left in points]
        signals = spread_observations(points, lookback_bars=4, horizon_bars=2,
                                      entry_z=2.0, hedge_ratio=1.0)
        self.assertEqual(len(signals), 1)
        self.assertTrue(signals[0]["converged"])

    def test_summary_does_not_promote_empty_history(self):
        result = summarize([], min_observations=2, min_convergence_bps=0.0, paper_cost_bps=0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertEqual(result["signals"], 0)


if __name__ == "__main__":
    unittest.main()
