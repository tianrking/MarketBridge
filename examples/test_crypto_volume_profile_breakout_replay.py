#!/usr/bin/env python3
"""Deterministic tests for volume-profile replay."""

import unittest

from crypto_volume_profile_breakout_replay import profile_features, summarize


class VolumeProfileTests(unittest.TestCase):
    def test_profile_features_exposes_value_area_and_low_nodes(self):
        rows = []
        for index in range(10):
            price = 100.0 + (index % 3)
            rows.append({"ts_ms": index * 60_000, "close": price,
                         "high": price + 0.1, "low": price - 0.1,
                         "volume": 100.0 if index % 3 == 0 else 10.0})
        features = profile_features(rows, 9, 9, 6, 0.7, 0.25)
        self.assertIsNotNone(features)
        self.assertLessEqual(features["value_low"], features["value_high"])
        self.assertGreaterEqual(features["low_node_count"], 1)

    def test_summary_applies_paper_cost(self):
        summary = summarize([{"aligned_return_bps": 20.0, "aligned": True},
                             {"aligned_return_bps": 10.0, "aligned": True}], 2, 5.0, 0.0)
        self.assertEqual(summary["mean_cost_adjusted_return_bps"], 10.0)
        self.assertEqual(summary["verdict"], "low_volume_node_breakout_candidate")


if __name__ == "__main__":
    unittest.main()
