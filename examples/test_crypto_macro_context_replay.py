#!/usr/bin/env python3
"""Deterministic tests for the macro-context response replay."""

import unittest

from crypto_macro_context_replay import summarize_records


def record(price, timestamp, context_state="elevated_volatility_context",
           funding_state="long_crowding_context"):
    return {"recorded_at_ms": timestamp, "observation": {
        "price": {"price": price}, "context_state": context_state,
        "funding": {"state": funding_state},
    }}


class MacroReplayTests(unittest.TestCase):
    def test_context_and_funding_buckets_keep_forward_response(self):
        result = summarize_records([record(100, 1), record(99, 2), record(98, 3)], 2, 1, 0)
        bucket = result["by_context_and_funding"]["elevated_volatility_context|long_crowding_context"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], -2.0)
        self.assertEqual(bucket["downside_fraction"], 1.0)

    def test_missing_price_does_not_become_zero(self):
        rows = [record(100, 1), {"recorded_at_ms": 2, "observation": {"context_state": "normal_volatility_context"}}, record(102, 3)]
        result = summarize_records(rows, 1, 1, 0)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_macro_observations")

    def test_funding_state_separates_same_macro_state(self):
        rows = [record(100, 1, funding_state="short_crowding_context"),
                record(101, 2, funding_state="normal_funding_context"),
                record(102, 3, funding_state="normal_funding_context")]
        result = summarize_records(rows, 2, 1, 0)
        self.assertIn("elevated_volatility_context|short_crowding_context",
                      result["by_context_and_funding"])


if __name__ == "__main__":
    unittest.main()
