#!/usr/bin/env python3
"""Deterministic tests for spot/perp depth-gap persistence replay."""

import unittest

from crypto_spot_perp_depth_gap_replay import summarize_records


def record(ratio, improvement, state="perp_depth_advantage_observation"):
    return {"observation": {
        "state": state,
        "basis_bps": 2.0,
        "spot": {"metrics": {
            "bid_depth_notional": 100.0, "ask_depth_notional": 100.0,
            "buy_impact_bps": 10.0, "sell_impact_bps": 10.0,
        }},
        "perp": {"metrics": {
            "bid_depth_notional": ratio * 100.0,
            "ask_depth_notional": ratio * 100.0,
            "buy_impact_bps": 10.0 - improvement,
            "sell_impact_bps": 10.0 - improvement,
        }},
    }}


class SpotPerpDepthReplayTests(unittest.TestCase):
    def test_persistent_advantage_requires_fraction_and_run(self):
        result = summarize_records(
            [record(3.0, 6.0), record(3.0, 6.0), record(3.0, 6.0),
             record(1.1, 1.0)],
            2.0, 5.0, 3,
        )
        self.assertEqual(result["verdict"], "persistent_perp_depth_advantage")
        self.assertEqual(result["perp_advantage_snapshots"], 3)
        self.assertEqual(result["longest_perp_advantage_run"], 3)

    def test_missing_depth_is_not_counted_as_no_gap(self):
        result = summarize_records([{"observation": {"spot": {}, "perp": {}}}], 2.0, 5.0, 1)
        self.assertEqual(result["valid_depth_snapshots"], 0)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
