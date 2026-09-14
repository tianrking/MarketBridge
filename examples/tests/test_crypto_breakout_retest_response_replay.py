"""Deterministic tests for breakout-retest response replay."""

import unittest

from crypto_breakout_retest_response_replay import (
    build_observations,
    prior_levels,
    summarize,
)


class BreakoutRetestResponseReplayTests(unittest.TestCase):
    def setUp(self):
        self.rows = [
            {"ts_ms": 0, "open": 95.0, "high": 100.0, "low": 90.0, "close": 95.0},
            {"ts_ms": 1, "open": 95.0, "high": 99.0, "low": 91.0, "close": 94.0},
            {"ts_ms": 2, "open": 94.0, "high": 98.0, "low": 92.0, "close": 93.0},
            {"ts_ms": 3, "open": 99.0, "high": 105.0, "low": 98.0, "close": 103.0},
            {"ts_ms": 4, "open": 103.0, "high": 104.0, "low": 100.0, "close": 102.0},
            {"ts_ms": 5, "open": 102.0, "high": 106.0, "low": 101.0, "close": 105.0},
        ]

    def test_prior_levels_exclude_breakout_and_retest_bars(self):
        levels = prior_levels(self.rows, 3, 3)
        self.assertEqual(levels, {"resistance": 100.0, "support": 90.0})

    def test_breakout_retest_response_starts_at_retest_close(self):
        observations = build_observations(self.rows, 3, 2, 15.0, 0.0, 1)
        self.assertEqual(len(observations), 1)
        self.assertEqual(observations[0]["breakout_ts_ms"], 3)
        self.assertEqual(observations[0]["retest_ts_ms"], 4)
        self.assertAlmostEqual(observations[0]["forward_return_pct"], 105.0 / 102.0 * 100.0 - 100.0)
        self.assertTrue(observations[0]["aligned"])

    def test_summary_applies_paper_cost(self):
        rows = [{"direction_sign": 1, "aligned_return_bps": 30.0, "aligned": True},
                {"direction_sign": -1, "aligned_return_bps": 20.0, "aligned": True}]
        result = summarize(rows, 5.0, 2, 10.0)
        self.assertEqual(result["mean_cost_adjusted_return_bps"], 20.0)
        self.assertEqual(result["verdict"], "breakout_retest_response_reported")


if __name__ == "__main__":
    unittest.main()
