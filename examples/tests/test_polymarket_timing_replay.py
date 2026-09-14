#!/usr/bin/env python3
"""Deterministic tests for Polymarket timing replay helpers."""

import unittest

from polymarket_timing_replay import bucket_name, parse_timestamp, summarize


class TimingReplayTests(unittest.TestCase):
    def test_timestamp_and_bucket_boundaries(self):
        self.assertEqual(parse_timestamp("2026-01-01T00:00:00Z"), 1767225600.0)
        self.assertEqual(bucket_name(10.0, 0.0, 30.0), "middle")
        self.assertEqual(bucket_name(30.0, 0.0, 30.0), "late")

    def test_summary_scores_fixed_stake_by_bucket(self):
        rows = [
            {"side": "BUY", "timestamp": 0, "price": 0.5, "size": 1, "outcome": "Yes"},
            {"side": "BUY", "timestamp": 10, "price": 0.5, "size": 1, "outcome": "No"},
            {"side": "BUY", "timestamp": 30, "price": 0.25, "size": 1, "outcome": "Yes"},
        ]
        result, missing = summarize(rows, "Yes", 0.0, 30.0, 10.0, 0.0)
        self.assertEqual(missing, 0)
        self.assertEqual(result["early"]["wins"], 1)
        self.assertEqual(result["middle"]["losses"], 1)
        self.assertEqual(result["late"]["wins"], 1)
        self.assertAlmostEqual(result["late"]["paper_pnl"], 30.0)


if __name__ == "__main__":
    unittest.main()
