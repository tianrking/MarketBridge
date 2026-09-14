"""Deterministic tests for cross-asset correlation response replay."""

import unittest

from crypto_cross_asset_correlation_response_replay import (
    build_observations,
    classify_correlation,
    pearson,
    summarize,
)


class CrossAssetCorrelationResponseTests(unittest.TestCase):
    def test_pearson_and_threshold_states_are_explicit(self):
        self.assertAlmostEqual(pearson([1.0, 2.0, 3.0], [2.0, 4.0, 6.0]), 1.0)
        self.assertEqual(classify_correlation(0.10, 0.30, 0.70), "low_correlation")
        self.assertEqual(classify_correlation(0.90, 0.30, 0.70), "high_correlation")
        self.assertEqual(classify_correlation(None, 0.30, 0.70), "missing_correlation")

    def test_exact_timestamp_intersection_and_forward_response(self):
        first = {index: 100.0 + index for index in range(9)}
        second = {index: 200.0 + index for index in range(9)}
        second.pop(4)
        timestamps, observations = build_observations(first, second, 2, 1, 0.30, 0.70)
        self.assertNotIn(4, timestamps)
        self.assertTrue(observations)
        self.assertIn("relative_forward_return_pct", observations[0])

    def test_future_value_does_not_change_current_correlation(self):
        first = {index: 100.0 + index for index in range(8)}
        second = {index: 200.0 + index for index in range(8)}
        _, before = build_observations(first, second, 3, 1, 0.30, 0.70)
        second[7] = 500.0
        _, after = build_observations(first, second, 3, 1, 0.30, 0.70)
        self.assertEqual(before[0]["correlation"], after[0]["correlation"])

    def test_summary_requires_low_and_high_regimes(self):
        rows = [
            {"state": "low_correlation", "correlation": 0.1,
             "relative_forward_return_pct": 1.0, "absolute_relative_return_pct": 1.0},
            {"state": "high_correlation", "correlation": 0.9,
             "relative_forward_return_pct": 0.2, "absolute_relative_return_pct": 0.2},
        ]
        result = summarize(rows, 1, 10.0)
        self.assertEqual(result["absolute_relative_edge_bps_low_minus_high"], 80.0)
        self.assertEqual(result["verdict"], "cross_asset_correlation_response_reported")


if __name__ == "__main__":
    unittest.main()
