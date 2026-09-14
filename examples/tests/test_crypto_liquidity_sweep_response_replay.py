"""Deterministic tests for liquidity-sweep response replay."""

import unittest

from crypto_liquidity_sweep_response_replay import (
    build_observations,
    prior_range,
    summarize,
    sweep_signal,
)


class LiquiditySweepResponseReplayTests(unittest.TestCase):
    def setUp(self):
        self.rows = [
            {"ts_ms": 0, "open": 95.0, "high": 100.0, "low": 90.0, "close": 95.0},
            {"ts_ms": 1, "open": 95.0, "high": 99.0, "low": 91.0, "close": 94.0},
            {"ts_ms": 2, "open": 94.0, "high": 98.0, "low": 92.0, "close": 93.0},
            {"ts_ms": 3, "open": 91.0, "high": 101.0, "low": 88.0, "close": 99.0},
            {"ts_ms": 4, "open": 99.0, "high": 102.0, "low": 97.0, "close": 100.0},
        ]

    def test_prior_range_excludes_trigger_and_future_bars(self):
        levels = prior_range(self.rows, 3, 3)
        self.assertEqual(levels["high"], 100.0)
        self.assertEqual(levels["low"], 90.0)
        signal = sweep_signal(self.rows[3], levels, 0.0, 0.5, 5.0)
        self.assertEqual(signal["direction"], "bullish")
        self.assertEqual(signal["sweep_level"], 90.0)

    def test_build_observations_measures_aligned_forward_response(self):
        observations = build_observations(self.rows, 3, 1, 0.0, 0.5, 5.0)
        self.assertEqual(len(observations), 1)
        self.assertAlmostEqual(observations[0]["forward_return_pct"], 100.0 / 99.0)
        self.assertTrue(observations[0]["aligned"])

    def test_summary_applies_cost_hurdle(self):
        observations = [{"signal": {"direction_sign": 1},
                         "aligned_return_bps": 25.0, "aligned": True},
                        {"signal": {"direction_sign": -1},
                         "aligned_return_bps": 15.0, "aligned": True}]
        result = summarize(observations, 5.0, 2, 10.0)
        self.assertEqual(result["mean_cost_adjusted_return_bps"], 15.0)
        self.assertEqual(result["verdict"], "liquidity_sweep_response_reported")


if __name__ == "__main__":
    unittest.main()
