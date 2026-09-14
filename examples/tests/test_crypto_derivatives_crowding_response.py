#!/usr/bin/env python3
"""Deterministic tests for the aggregate crowding-response replay."""

import unittest

from crypto_derivatives_crowding_response_replay import summarize_records


def record(state, value, timestamp, liquidation=False):
    return {"recorded_at_ms": timestamp, "observation": {
        "summary": {"state": state, "liquidation_activity": liquidation},
        "price": {"price": value},
    }}


class CrowdingResponseTests(unittest.TestCase):
    def test_long_crowding_is_signed_contrarian(self):
        result = summarize_records([
            record("long_crowding_context", 100, 1),
            record("mixed_or_neutral_positioning_context", 98, 2),
            record("mixed_or_neutral_positioning_context", 95, 3),
        ], 2, 1, 0)
        self.assertAlmostEqual(
            result["contrarian_buckets"]["long_crowding_context"]["mean_signed_return_pct"],
            5.0,
        )

    def test_short_crowding_with_liquidation_bucket(self):
        result = summarize_records([
            record("short_crowding_context", 100, 1, True),
            record("mixed_or_neutral_positioning_context", 101, 2),
            record("mixed_or_neutral_positioning_context", 102, 3),
        ], 2, 1, 0)
        self.assertEqual(result["contrarian_buckets"]["short_crowding_with_liquidation"]["observations"], 1)
        self.assertAlmostEqual(result["contrarian_buckets"]["short_crowding_context"]["mean_signed_return_pct"], 2.0)

    def test_missing_prices_do_not_become_zero(self):
        result = summarize_records([
            record("long_crowding_context", 100, 1),
            {"recorded_at_ms": 2, "observation": {"summary": {"state": "long_crowding_context"}}},
            record("long_crowding_context", 90, 3),
        ], 1, 1, 0)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_crowding_observations")


if __name__ == "__main__":
    unittest.main()
