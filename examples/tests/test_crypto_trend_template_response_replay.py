"""Deterministic tests for the price-only trend-template response replay."""

import unittest

from crypto_trend_template_response_replay import (
    aligned_observations,
    classify_template,
    summarize,
)


class TrendTemplateResponseReplayTests(unittest.TestCase):
    def test_template_requires_ordered_averages_and_range_position(self):
        self.assertEqual(
            classify_template(120, 115, 110, 100, 99, 125, 80, 0.75, 1.3),
            "trend_template_pass",
        )
        self.assertEqual(
            classify_template(90, 95, 100, 105, 106, 125, 80, 0.75, 1.3),
            "ordinary_trend_state",
        )

    def test_alignment_uses_only_prior_average_and_range_windows(self):
        prices = [
            (f"2024-01-0{index + 1}", (100.0 + index, index))
            for index in range(8)
        ]
        rows = aligned_observations(prices, 2, 3, 4, 1, 4, 0.75, 1.0, 1)
        self.assertTrue(rows)
        self.assertIn("forward_min_path_return_pct", rows[0])
        self.assertEqual(rows[1]["state"], "trend_template_pass")

    def test_summary_is_observe_only_with_short_pass_sample(self):
        result = summarize([{
            "state": "trend_template_pass",
            "forward_return_pct": 1.0,
            "forward_min_path_return_pct": -1.0,
        }], 2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_trend_template_windows")


if __name__ == "__main__":
    unittest.main()
