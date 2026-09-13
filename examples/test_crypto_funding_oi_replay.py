#!/usr/bin/env python3
"""Deterministic tests for funding/OI crowding replay."""

import unittest

from crypto_funding_oi_replay import classify_state, forward_return, summarize


class FundingOiReplayTests(unittest.TestCase):
    def test_crowding_state_requires_rising_oi(self):
        self.assertEqual(classify_state(0.02, 0.20, 0.01, 0.10), "long_crowded")
        self.assertEqual(classify_state(-0.02, 0.20, 0.01, 0.10), "short_crowded")
        self.assertEqual(classify_state(0.02, 0.0, 0.01, 0.10), "other")
        self.assertEqual(classify_state(0.02, None, 0.01, 0.10), "missing_oi")

    def test_forward_return_and_expected_direction_summary(self):
        prices = [(0, 100.0), (5, 99.0), (10, 98.0), (15, 101.0)]
        self.assertAlmostEqual(forward_return(5, prices, 2), 2.0202020202)
        rows = [
            {"state": "long_crowded", "forward_return_pct": -1.0},
            {"state": "long_crowded", "forward_return_pct": 1.0},
            {"state": "short_crowded", "forward_return_pct": 2.0},
        ]
        result = summarize(rows)
        self.assertEqual(result["long_crowded"]["observations"], 2)
        self.assertAlmostEqual(result["long_crowded"]["expected_direction_hit_rate"], 0.5)
        self.assertEqual(result["short_crowded"]["expected_direction_hit_rate"], 1.0)


if __name__ == "__main__":
    unittest.main()
