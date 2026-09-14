"""Deterministic tests for Fibonacci retracement response replay."""

import unittest

from crypto_fibonacci_retracement_response_replay import (
    build_observations,
    classify_ratio,
    retracement_ratio,
    swing_levels,
    summarize,
)


class FibonacciRetracementResponseReplayTests(unittest.TestCase):
    def setUp(self):
        self.rows = [
            {"ts_ms": 0, "high": 100.0, "low": 90.0, "close": 95.0},
            {"ts_ms": 1, "high": 105.0, "low": 92.0, "close": 103.0},
            {"ts_ms": 2, "high": 110.0, "low": 100.0, "close": 108.0},
            {"ts_ms": 3, "high": 108.0, "low": 100.0, "close": 103.0},
            {"ts_ms": 4, "high": 106.0, "low": 101.0, "close": 104.0},
        ]

    def test_swing_levels_are_as_of_and_direction_aware(self):
        levels = swing_levels(self.rows, 3, 3)
        self.assertEqual(levels["high"], 110.0)
        self.assertEqual(levels["low"], 90.0)
        self.assertEqual(levels["direction"], 1)
        self.assertAlmostEqual(retracement_ratio(103.0, levels), 0.35)

    def test_ratio_classification_uses_explicit_tolerance(self):
        self.assertEqual(classify_ratio(0.382, 0.01), "fib_382")
        self.assertEqual(classify_ratio(0.50, 0.01), "fib_500")
        self.assertEqual(classify_ratio(1.10, 0.03), "outside_swing_range")

    def test_summary_reports_level_buckets(self):
        observations = build_observations(self.rows, 3, 0.05, 1)
        self.assertEqual(len(observations), 1)
        result = summarize(observations, 1)
        self.assertEqual(result["verdict"], "fibonacci_response_reported")
        self.assertIn(observations[0]["state"], result["by_state"])


if __name__ == "__main__":
    unittest.main()
