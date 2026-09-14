"""Deterministic tests for StochRSI response replay."""

import unittest

from crypto_stochrsi_response_replay import (
    build_observations,
    rsi_series,
    stochrsi_features,
    stochrsi_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class StochRsiResponseTests(unittest.TestCase):
    def test_rsi_and_stochrsi_warmup_are_explicit(self):
        rows = [bar(index, 100.0 + index) for index in range(50)]
        rsi = rsi_series(rows, 3)
        self.assertIsNone(rsi[2])
        self.assertEqual(rsi[3], 100.0)
        series = stochrsi_series(rows, 3, 3, 2, 2)
        self.assertIsNone(series[7])
        self.assertIsNone(stochrsi_features(rows, 0, 3, 3, 2, 2))

    def test_constant_rsi_window_is_not_filled(self):
        rows = [bar(index, 100.0) for index in range(50)]
        self.assertIsNone(stochrsi_series(rows, 3, 3, 2, 2)[-1])

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(50)]
        before = stochrsi_features(rows, 30, 4, 3, 3)
        rows[-1] = bar(49, 1000.0, 1001.0, 999.0)
        after = stochrsi_features(rows, 30, 4, 3, 3)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_extreme_and_control(self):
        rows = [bar(index, 100.0 + float((index % 10) - 5) ** 2) for index in range(80)]
        observations = build_observations(rows, 3, 3, 2, 2, 80.0, 20.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"state": "oversold", "event": "bullish_kd_cross", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "neutral", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "stochrsi_response_reported")
        self.assertEqual(result["kd_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
