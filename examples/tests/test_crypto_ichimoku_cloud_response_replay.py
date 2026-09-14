"""Deterministic tests for as-of Ichimoku response replay."""

import unittest

from crypto_ichimoku_cloud_response_replay import (
    build_observations,
    classify_state,
    ichimoku_at,
    summarize,
)


class IchimokuCloudResponseReplayTests(unittest.TestCase):
    def test_cloud_uses_displaced_historical_spans(self):
        rows = []
        for index in range(12):
            close = 100.0 + index
            rows.append({"ts_ms": index, "open": close, "high": close + 1,
                         "low": close - 1, "close": close})
        values = ichimoku_at(rows, 10, 2, 3, 4, 2)
        self.assertIsNotNone(values)
        self.assertEqual(values["past_close"], rows[8]["close"])
        self.assertLess(values["span_a"], (rows[9]["high"] + rows[9]["low"]) / 2.0)

    def test_state_requires_multiple_components(self):
        values = {"cloud_top": 100.0, "cloud_bottom": 90.0, "tenkan": 105.0,
                  "kijun": 100.0, "span_a": 102.0, "span_b": 98.0, "past_close": 95.0}
        self.assertEqual(classify_state(110.0, values, 0.0), "bullish_alignment")
        self.assertEqual(classify_state(110.0, {**values, "tenkan": 95.0}, 0.0), "above_cloud_mixed")
        self.assertEqual(classify_state(95.0, {**values, "cloud_top": 100.0, "cloud_bottom": 90.0,
                                                "tenkan": 85.0, "kijun": 90.0,
                                                "span_a": 88.0, "span_b": 92.0,
                                                "past_close": 100.0}, 0.0), "inside_cloud")

    def test_build_observations_and_summary(self):
        rows = []
        for index in range(24):
            close = 100.0 + index * 0.5
            rows.append({"ts_ms": index, "open": close, "high": close + 1,
                         "low": close - 1, "close": close})
        observations = build_observations(rows, 2, 3, 4, 2, 0.0, 2)
        self.assertTrue(observations)
        result = summarize(observations, 2)
        self.assertEqual(result["verdict"], "ichimoku_response_reported")


if __name__ == "__main__":
    unittest.main()
