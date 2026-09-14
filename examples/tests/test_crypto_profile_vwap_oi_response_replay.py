"""Deterministic tests for profile/VWAP/OI confluence replay."""

import unittest

from crypto_profile_vwap_oi_response_replay import (
    classify_state,
    interval_millis,
    oi_change_at,
    profile_vwap_features,
    summarize,
)


def bar(index, close, volume=10.0):
    return {"ts_ms": index * 3_600_000, "high": close + 1.0,
            "low": close - 1.0, "close": close, "volume": volume}


class ProfileVwapOiTests(unittest.TestCase):
    def test_interval_parser_and_point_in_time_features(self):
        self.assertEqual(interval_millis("1h"), 3_600_000)
        self.assertEqual(interval_millis("15m"), 900_000)
        self.assertIsNone(interval_millis("1M"))
        rows = [bar(index, 100.0 + index) for index in range(6)]
        features = profile_vwap_features(rows, 4, 3, 6, 0.70, 0.25)
        self.assertAlmostEqual(features["vwap"], (101 + 102 + 103) / 3)
        mutated = list(rows)
        mutated[5] = bar(5, 10_000.0, 1_000_000.0)
        self.assertEqual(features, profile_vwap_features(mutated, 4, 3, 6, 0.70, 0.25))

    def test_oi_change_is_asof_and_staleness_is_visible(self):
        points = [(0, 100.0), (3_600_000, 110.0), (7_200_000, 121.0)]
        result = oi_change_at(points, 7_300_000, 3_600_000, 200_000)
        self.assertAlmostEqual(result["change_pct"], 10.0)
        self.assertEqual(result["age_ms"], 100_000)
        self.assertIsNone(oi_change_at(points, 7_500_000, 3_600_000, 200_000))

    def test_confluence_and_control_states_remain_separate(self):
        features = {"value_low": 99.0, "value_high": 101.0, "vwap": 100.0}
        self.assertEqual(classify_state(102.0, features, 0.20, 0.10),
                         "bullish_profile_vwap_oi")
        self.assertEqual(classify_state(98.0, features, -0.20, 0.10),
                         "bearish_profile_vwap_oi")
        self.assertEqual(classify_state(102.0, features, 0.0, 0.10),
                         "profile_vwap_aligned_without_oi")
        self.assertEqual(classify_state(100.0, features, 0.20, 0.10),
                         "inside_value_area")

    def test_summary_requires_confluence_and_control(self):
        rows = [
            {"state": "bullish_profile_vwap_oi", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "inside_value_area", "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(rows, 1)
        self.assertEqual(result["verdict"], "profile_vwap_oi_confluence_reported")
        self.assertEqual(result["confluence_observations"], 1)
        self.assertEqual(result["control_observations"], 1)


if __name__ == "__main__":
    unittest.main()
