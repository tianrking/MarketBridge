"""Deterministic tests for CCI response replay."""

import unittest

from crypto_cci_response_replay import (
    build_observations,
    cci_features,
    cci_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class CciResponseTests(unittest.TestCase):
    def test_typical_price_mean_deviation_and_warmup(self):
        rows = [bar(index, 100.0 + index * 2, 101.0 + index * 2, 99.0 + index * 2)
                for index in range(6)]
        series = cci_series(rows, 3, 0.015, 100.0, -100.0)
        self.assertIsNone(series[1])
        self.assertIsNotNone(series[2])
        self.assertAlmostEqual(series[2]["cci"], 100.0)
        self.assertIsNone(cci_features(rows, 0, 3))

    def test_zero_deviation_is_not_filled(self):
        rows = [bar(index, 100.0, 100.0, 100.0) for index in range(8)]
        self.assertIsNone(cci_series(rows, 3)[-1])

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(12)]
        before = cci_features(rows, 6, 4)
        rows[-1] = bar(11, 1000.0, 1001.0, 999.0)
        after = cci_features(rows, 6, 4)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_extreme_and_control(self):
        rows = [bar(index, 100.0 + (index % 8) * 1.5) for index in range(40)]
        observations = build_observations(rows, 4, 0.015, 100.0, -100.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])
        manual = [
            {"state": "oversold", "event": "positive_zero_cross", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "positive_neutral", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": 10.0, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "cci_response_reported")
        self.assertEqual(result["cci_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
