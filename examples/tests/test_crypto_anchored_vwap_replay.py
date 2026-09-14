#!/usr/bin/env python3
"""Deterministic tests for the anchored-VWAP replay."""

import unittest

from crypto_anchored_vwap_replay import anchored_vwap_observations, summarize


def candle(timestamp, close, volume=10.0):
    return {"ts_ms": timestamp, "close": close, "high": close + 1,
            "low": close - 1, "volume": volume}


class AnchoredVwapTests(unittest.TestCase):
    def test_low_anchor_reclaim_is_long_and_aligned(self):
        rows = [candle(i, value) for i, value in enumerate(
            [100, 95, 90, 89, 90, 92, 88, 100, 105, 107, 108]
        )]
        observations = anchored_vwap_observations(rows, 3, "low", 0, 0, 2)
        self.assertTrue(any(row["anchor_type"] == "low" and row["direction"] == "long"
                            and row["aligned"] for row in observations))

    def test_high_anchor_rejection_is_short(self):
        rows = [candle(i, value) for i, value in enumerate(
            [100, 105, 110, 111, 110, 108, 112, 100, 95, 93, 92]
        )]
        observations = anchored_vwap_observations(rows, 3, "high", 0, 0, 2)
        self.assertTrue(any(row["anchor_type"] == "high" and row["direction"] == "short"
                            for row in observations))

    def test_volume_requirement_keeps_missing_volume_out(self):
        rows = [candle(i, value, None) for i, value in enumerate([100, 95, 96, 97, 98, 104, 106])]
        self.assertEqual(anchored_vwap_observations(rows, 3, "low", 0, 1, 2), [])

    def test_summary_requires_minimum_observations(self):
        result = summarize([{"aligned_return_bps": 20.0, "aligned": True, "anchor_type": "low"}], 2, 5, 0)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
