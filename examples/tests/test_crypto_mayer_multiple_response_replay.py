"""Deterministic tests for the Mayer Multiple response replay."""

import unittest

from crypto_mayer_multiple_response_replay import (
    aligned_observations,
    classify_multiple,
    summarize,
)


class MayerMultipleResponseReplayTests(unittest.TestCase):
    def test_thresholds_keep_deep_discount_and_extreme_premium_distinct(self):
        self.assertEqual(classify_multiple(0.55, 0.8, 0.6, 2.4, 3.0), "deep_discount")
        self.assertEqual(classify_multiple(2.8, 0.8, 0.6, 2.4, 3.0), "premium")
        self.assertEqual(classify_multiple(3.1, 0.8, 0.6, 2.4, 3.0), "extreme_premium")

    def test_alignment_uses_only_prior_close_window_and_later_price(self):
        prices = [
            (f"2024-01-0{index + 1}", (100.0 + index * 10.0, index))
            for index in range(6)
        ]
        rows = aligned_observations(prices, 3, 0.8, 0.6, 2.4, 3.0, 1)
        self.assertEqual(rows[0]["date"], "2024-01-03")
        self.assertAlmostEqual(rows[0]["sma_close"], 110.0)
        self.assertAlmostEqual(rows[0]["mayer_multiple"], 120.0 / 110.0)
        self.assertAlmostEqual(rows[0]["forward_return_pct"], (130.0 / 120.0 - 1.0) * 100.0)

    def test_summary_stays_observe_only_with_few_extreme_windows(self):
        result = summarize([{
            "state": "deep_discount",
            "mayer_multiple": 0.55,
            "forward_return_pct": 1.0,
            "forward_abs_return_pct": 1.0,
        }], 2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_mayer_extreme_windows")


if __name__ == "__main__":
    unittest.main()
