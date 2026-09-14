"""Deterministic tests for Parabolic SAR response replay."""

import unittest

from crypto_parabolic_sar_response_replay import (
    build_observations,
    psar_features,
    psar_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class ParabolicSarResponseTests(unittest.TestCase):
    def test_two_bar_initialization_and_rising_direction(self):
        rows = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                for index in range(10)]
        series = psar_series(rows, 0.02, 0.02, 0.2)
        self.assertIsNone(series[0])
        self.assertEqual(series[1]["state"], "bullish_trend")
        self.assertEqual(series[-1]["direction_sign"], 1)
        self.assertIsNone(psar_features(rows, 0))

    def test_reversal_flip_is_retained(self):
        rows = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                for index in range(8)]
        rows.extend([bar(8, 104.0, 105.0, 103.0), bar(9, 94.0, 95.0, 93.0),
                     bar(10, 90.0, 91.0, 89.0)])
        series = psar_series(rows, 0.02, 0.02, 0.2)
        self.assertTrue(any(item and item["event"] == "bearish_flip" for item in series))

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(12)]
        before = psar_features(rows, 6)
        rows[-1] = bar(11, 1000.0, 1001.0, 999.0)
        after = psar_features(rows, 6)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_flip_and_controls(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(30)]
        observations = build_observations(rows, 0.02, 0.02, 0.2, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])
        manual = [
            {"state": "bullish_trend", "event": "bullish_flip", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "bearish_trend", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": -10.0, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "parabolic_sar_response_reported")
        self.assertEqual(result["flip_observations"], 1)


if __name__ == "__main__":
    unittest.main()
