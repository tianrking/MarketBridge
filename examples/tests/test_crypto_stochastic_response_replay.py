"""Deterministic tests for Stochastic response replay."""

import unittest

from crypto_stochastic_response_replay import (
    build_observations,
    stochastic_features,
    stochastic_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class StochasticResponseTests(unittest.TestCase):
    def test_smoothing_warmup_and_high_close_are_explicit(self):
        rows = [bar(index, 100.0 + index, 100.0 + index, 98.0 + index) for index in range(12)]
        series = stochastic_series(rows, 3, 2, 2, 80.0, 20.0)
        self.assertIsNone(series[3])
        self.assertIsNotNone(series[5])
        self.assertAlmostEqual(series[5]["k"], 100.0)
        self.assertEqual(series[5]["state"], "overbought")
        self.assertIsNone(stochastic_features(rows, 0, 3, 2, 2))

    def test_zero_range_is_not_filled(self):
        rows = [bar(index, 100.0, 100.0, 100.0) for index in range(10)]
        self.assertIsNone(stochastic_series(rows, 3, 2, 2)[-1])

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(15)]
        before = stochastic_features(rows, 8, 4, 3, 3)
        rows[-1] = bar(14, 1000.0, 1001.0, 999.0)
        after = stochastic_features(rows, 8, 4, 3, 3)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_extreme_and_control(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(40)]
        observations = build_observations(rows, 3, 2, 2, 80.0, 20.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])
        manual = [
            {"state": "oversold", "event": "bullish_kd_cross", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "neutral", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "stochastic_response_reported")
        self.assertEqual(result["kd_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
