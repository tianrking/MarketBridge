#!/usr/bin/env python3
"""Deterministic tests for session confluence replay."""

import unittest
from datetime import time

from crypto_session_momentum_replay import confluence_observations, parse_clock, summarize


class SessionMomentumTests(unittest.TestCase):
    def test_parse_clock(self):
        self.assertEqual(parse_clock("09:15"), time(9, 15))
        with self.assertRaises(ValueError):
            parse_clock("bad")

    def test_session_confluence_uses_future_timestamp(self):
        rows = []
        start = 0
        for index in range(30):
            close = 100.0 + index * 0.1
            rows.append({"ts_ms": start + index * 60_000, "close": close,
                         "high": close + 0.1, "low": close - 0.1, "volume": 100.0})
        rows[21]["volume"] = 200.0
        observations = confluence_observations(rows, "UTC", time(0, 20), time(0, 22), 1.0, 1, 4)
        self.assertTrue(observations)
        self.assertEqual(observations[0]["direction"], "long")
        self.assertGreater(observations[0]["aligned_return_bps"], 0)

    def test_missing_future_bar_is_excluded(self):
        rows = [{"ts_ms": 0, "close": 100.0, "high": 101.0, "low": 99.0, "volume": 100.0}]
        self.assertEqual(confluence_observations(rows, "UTC", time(0, 0), time(0, 1), 1.0, 1, 4), [])

    def test_summary_cost_hurdle(self):
        summary = summarize([{"aligned_return_bps": 12.0, "aligned": True},
                             {"aligned_return_bps": 8.0, "aligned": True}], 2, 5.0, 0.0)
        self.assertEqual(summary["verdict"], "session_confluence_candidate")
        self.assertEqual(summary["mean_cost_adjusted_return_bps"], 5.0)


if __name__ == "__main__":
    unittest.main()
