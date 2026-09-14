"""Deterministic tests for ATR regime response replay."""

import unittest

from crypto_atr_regime_response_replay import (
    atr_series,
    build_observations,
    classify_atr,
    summarize,
)


class AtrRegimeResponseReplayTests(unittest.TestCase):
    def test_atr_uses_current_and_prior_true_ranges(self):
        rows = [
            {"close": 100.0, "high": 101.0, "low": 99.0},
            {"close": 102.0, "high": 104.0, "low": 100.0},
            {"close": 101.0, "high": 103.0, "low": 99.0},
        ]
        values = atr_series(rows, 2)
        self.assertIsNone(values[0])
        self.assertAlmostEqual(values[2], 4.0)

    def test_regime_classification_is_as_of(self):
        self.assertEqual(classify_atr(1.0, [1.0, 2.0, 3.0, 4.0], 0.25, 0.75)[0], "compressed")
        self.assertEqual(classify_atr(4.0, [1.0, 2.0, 3.0, 4.0], 0.25, 0.75)[0], "expanded")
        self.assertEqual(classify_atr(2.5, [1.0, 2.0, 3.0, 4.0], 0.25, 0.75)[0], "ordinary")

    def test_observations_do_not_use_future_atr_for_thresholds(self):
        rows = []
        for index, close in enumerate((100, 101, 100, 101, 100, 101, 100, 101, 130)):
            rows.append({"ts_ms": index, "open": close, "close": close,
                         "high": close + 1, "low": close - 1})
        observations = build_observations(rows, 2, 3, 0.25, 0.75, 1)
        self.assertTrue(observations)
        self.assertNotEqual(observations[-1]["ts_ms"], 8)

    def test_summary_reports_signed_and_absolute_buckets(self):
        rows = [
            {"state": "compressed", "forward_return_pct": 1.0,
             "forward_absolute_return_pct": 1.0, "forward_min_path_return_pct": -0.2},
            {"state": "expanded", "forward_return_pct": -2.0,
             "forward_absolute_return_pct": 2.0, "forward_min_path_return_pct": -2.2},
        ]
        result = summarize(rows, 2)
        self.assertEqual(result["verdict"], "atr_regime_response_reported")
        self.assertEqual(result["by_state"]["expanded"]["negative_forward_fraction"], 1.0)


if __name__ == "__main__":
    unittest.main()
