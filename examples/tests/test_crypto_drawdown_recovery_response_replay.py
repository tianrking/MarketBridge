"""Deterministic tests for drawdown/recovery response replay."""

import unittest

from crypto_drawdown_recovery_response_replay import (
    build_observations,
    classify_drawdown,
    summarize,
)


class DrawdownRecoveryResponseReplayTests(unittest.TestCase):
    def test_classification_uses_ordered_positive_thresholds(self):
        self.assertEqual(classify_drawdown(0.0, 10.0, 20.0, 40.0), "near_ath")
        self.assertEqual(classify_drawdown(-10.0, 10.0, 20.0, 40.0), "mild_drawdown")
        self.assertEqual(classify_drawdown(-20.0, 10.0, 20.0, 40.0), "moderate_drawdown")
        self.assertEqual(classify_drawdown(-40.0, 10.0, 20.0, 40.0), "deep_drawdown")

    def test_running_high_does_not_look_into_the_future(self):
        prices = [(0, 100.0), (1, 110.0), (2, 100.0), (3, 120.0), (4, 125.0)]
        rows = build_observations(prices, 1, 10.0, 20.0, 40.0)
        drawdown_row = rows[2]
        self.assertEqual(drawdown_row["running_high"], 110.0)
        self.assertAlmostEqual(drawdown_row["drawdown_pct"], -9.0909090909)
        self.assertTrue(drawdown_row["recovered_prior_high"])

    def test_summary_reports_recovery_by_drawdown_state(self):
        prices = [(0, 100.0), (1, 60.0), (2, 70.0), (3, 110.0), (4, 50.0), (5, 55.0)]
        rows = build_observations(prices, 1, 10.0, 20.0, 40.0)
        result = summarize(rows, 2)
        self.assertEqual(result["verdict"], "drawdown_recovery_response_reported")
        self.assertEqual(result["by_state"]["moderate_drawdown"]["observations"], 1)
        self.assertEqual(result["by_state"]["deep_drawdown"]["recovery_fraction"], 0.0)


if __name__ == "__main__":
    unittest.main()
