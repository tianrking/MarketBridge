"""Deterministic tests for CMF response replay."""

import unittest

from crypto_cmf_response_replay import (
    build_observations,
    cmf_features,
    cmf_series,
    summarize,
)


def bar(ts_ms, close, volume=10.0, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close, "volume": volume}


class CmfResponseTests(unittest.TestCase):
    def test_close_location_multiplier_is_explicit(self):
        rows = [bar(0, 100.0, high=110.0, low=90.0),
                bar(1, 109.0, high=110.0, low=90.0)]
        series = cmf_series(rows, 2, 0.05, -0.05)
        self.assertAlmostEqual(series[1]["money_flow_multiplier"], 0.9)
        self.assertGreater(series[1]["cmf"], 0.0)
        self.assertIsNone(cmf_features(rows, 0, 2))

    def test_zero_range_is_not_filled_as_pressure(self):
        rows = [bar(index, 100.0, high=100.0, low=100.0) for index in range(4)]
        self.assertIsNone(cmf_series(rows, 3)[-1])

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5, volume=10.0) for index in range(10)]
        before = cmf_features(rows, 5, 3)
        rows[-1] = bar(9, 1000.0, volume=1000.0, high=1001.0, low=999.0)
        after = cmf_features(rows, 5, 3)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_pressure_and_neutral_control(self):
        rows = [bar(index, 100.0 + index * 0.2, volume=10.0) for index in range(30)]
        observations = build_observations(rows, 4, 0.05, -0.05, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"state": "positive_pressure", "event": "positive_zero_cross",
             "forward_return_pct": 1.0, "direction_aligned_return_bps": 100.0,
             "forward_absolute_return_pct": 1.0},
            {"state": "neutral", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "cmf_response_reported")
        self.assertEqual(result["zero_cross_observations"], 1)


if __name__ == "__main__":
    unittest.main()
