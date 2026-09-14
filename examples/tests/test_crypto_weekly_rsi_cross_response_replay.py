"""Deterministic tests for the weekly RSI crossover response replay."""

import unittest

from crypto_weekly_rsi_cross_response_replay import (
    aligned_observations,
    classify_cross,
    simple_rsi,
    summarize,
)


class WeeklyRsiCrossResponseReplayTests(unittest.TestCase):
    def test_rsi_warmup_and_constant_series_are_explicit(self):
        values = simple_rsi([100.0, 101.0, 102.0, 103.0], 2)
        self.assertEqual(values[:2], [None, None])
        self.assertEqual(values[2:], [100.0, 100.0])

    def test_cross_states_keep_directional_events_separate(self):
        self.assertEqual(classify_cross(60.0, 55.0, 50.0, 55.0), "rsi_cross_below_sma")
        self.assertEqual(classify_cross(50.0, 55.0, 60.0, 55.0), "rsi_cross_above_sma")

    def test_alignment_reports_future_path_minimum(self):
        prices = [
            (f"2024-01-0{index + 1}", (close, index))
            for index, close in enumerate([100.0, 90.0, 80.0, 85.0, 70.0, 75.0, 80.0, 85.0])
        ]
        rows = aligned_observations(prices, 2, 2, 1)
        self.assertTrue(rows)
        self.assertIn("forward_min_path_return_pct", rows[0])
        self.assertLessEqual(rows[0]["forward_min_path_return_pct"],
                             rows[0]["forward_return_pct"])

    def test_summary_stays_observe_only_with_short_cross_sample(self):
        result = summarize([{
            "state": "rsi_cross_below_sma",
            "forward_return_pct": -10.0,
            "forward_min_path_return_pct": -15.0,
        }], 2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_weekly_rsi_crosses")


if __name__ == "__main__":
    unittest.main()
