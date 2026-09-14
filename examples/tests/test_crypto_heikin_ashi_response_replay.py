"""Deterministic tests for Heikin-Ashi response replay."""

import unittest

from crypto_heikin_ashi_response_replay import (
    build_observations,
    heikin_ashi_features,
    heikin_ashi_series,
    summarize,
)


def bar(ts_ms, close, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class HeikinAshiResponseTests(unittest.TestCase):
    def test_first_bar_formula_and_bullish_wickless_state(self):
        rows = [bar(0, 100.0, 102.0, 100.0), bar(1, 102.0, 104.0, 102.0)]
        series = heikin_ashi_series(rows, 1.0, 5.0)
        self.assertAlmostEqual(series[0]["ha_close"], 100.5)
        self.assertEqual(series[0]["state"], "bullish_no_lower_wick")
        self.assertIsNone(heikin_ashi_features(rows, -1))

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5) for index in range(12)]
        before = heikin_ashi_features(rows, 6)
        rows[-1] = bar(11, 1000.0, 1001.0, 999.0)
        after = heikin_ashi_features(rows, 6)
        self.assertEqual(before, after)

    def test_observations_use_real_close_for_forward_response(self):
        rows = [bar(index, 100.0 + index * 0.2) for index in range(30)]
        observations = build_observations(rows, 1.0, 5.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_return_pct", observations[0])

    def test_summary_requires_wickless_and_controls(self):
        manual = [
            {"state": "bullish_no_lower_wick", "event": "bullish_ha_flip",
             "forward_return_pct": 1.0, "direction_aligned_return_bps": 100.0,
             "forward_absolute_return_pct": 1.0},
            {"state": "doji", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "heikin_ashi_response_reported")
        self.assertEqual(result["ha_flip_observations"], 1)


if __name__ == "__main__":
    unittest.main()
