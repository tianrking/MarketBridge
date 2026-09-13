#!/usr/bin/env python3
"""Deterministic tests for footprint imbalance research."""

import unittest

from crypto_footprint_imbalance_monitor import summarize_footprints
from crypto_footprint_imbalance_replay import summarize_records


def footprint(delta, bid_stack=None, ask_stack=None):
    return {"bucket_start_ms": 1, "bucket_end_ms": 2, "delta_notional": delta,
            "bid_notional": 100.0, "ask_notional": 200.0, "total_trades": 5,
            "stacked_imbalances_bid": bid_stack or [], "stacked_imbalances_ask": ask_stack or []}


def record(state, timestamp):
    return {"recorded_at_ms": timestamp, "observation": {"state": state}}


class FootprintTests(unittest.TestCase):
    def test_stacked_bid_pressure_overrides_weak_delta(self):
        result = summarize_footprints([footprint(1.0, [100, 101])], 0.2, 1)
        self.assertEqual(result["state"], "footprint_bid_pressure")

    def test_missing_footprint_is_visible(self):
        self.assertEqual(summarize_footprints([], 0.2, 1)["state"], "observe_only_missing_footprint")

    def test_replay_requires_consecutive_pressure(self):
        result = summarize_records([
            record("footprint_bid_pressure", 1), record("footprint_ask_pressure", 2),
            record("footprint_balanced", 3),
        ], 2)
        self.assertEqual(result["longest_pressure_run"], 2)
        self.assertEqual(result["verdict"], "persistent_footprint_pressure_candidate")


if __name__ == "__main__":
    unittest.main()
