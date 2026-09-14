"""Deterministic tests for the hash-ribbon response replay."""

import unittest
from datetime import date, timedelta

from crypto_hash_ribbon_response_replay import (
    aligned_observations,
    classify_cross,
    rolling_mean,
    summarize,
)


class HashRibbonResponseReplayTests(unittest.TestCase):
    def test_time_window_mean_requires_full_observed_span(self):
        points = [(0, 10.0), (86_400_000, 20.0), (2 * 86_400_000, 30.0)]
        self.assertAlmostEqual(rolling_mean(points, 2 * 86_400_000, 1), 25.0)
        self.assertIsNone(rolling_mean(points, 0, 2))

    def test_cross_classification_keeps_price_confirmation_explicit(self):
        self.assertEqual(
            classify_cross(90.0, 100.0, 110.0, 100.0, True),
            "hashrate_recovery_with_positive_price_momentum",
        )
        self.assertEqual(
            classify_cross(110.0, 100.0, 90.0, 100.0, False),
            "hashrate_capitulation_cross",
        )

    def test_alignment_measures_later_return_without_future_hashrate(self):
        day = 86_400_000
        base_ms = 1_704_067_200_000
        base_date = date(2024, 1, 1)
        hashrates = [(base_ms + index * day, 100.0 + index) for index in range(0, 8)]
        prices = [
            ((base_date + timedelta(days=index)).isoformat(),
             (100.0 + index, base_ms + index * day))
            for index in range(0, 8)
        ]
        rows = aligned_observations(hashrates, prices, 2, 4, 2, 3, 1)
        self.assertTrue(rows)
        self.assertIn(rows[-1]["state"], {"hashrate_expansion", "hashrate_balanced"})
        self.assertAlmostEqual(rows[0]["forward_return_pct"], (105.0 / 104.0 - 1.0) * 100.0)

    def test_summary_requires_recovery_sample(self):
        result = summarize([{
            "state": "hashrate_expansion",
            "forward_return_pct": 1.0,
            "forward_abs_return_pct": 1.0,
        }], 2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_hash_ribbon_crosses")


if __name__ == "__main__":
    unittest.main()
