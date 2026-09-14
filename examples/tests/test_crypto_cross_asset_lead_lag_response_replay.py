"""Deterministic tests for cross-asset lead-lag response replay."""

import unittest

from crypto_cross_asset_lead_lag_response_replay import (
    build_observations,
    classify_leader,
    summarize,
)


class CrossAssetLeadLagResponseReplayTests(unittest.TestCase):
    def test_leader_states_use_only_past_and_current_bars(self):
        self.assertEqual(classify_leader(1.0, 0.1), "leader_up")
        self.assertEqual(classify_leader(-1.0, 0.1), "leader_down")
        self.assertEqual(classify_leader(0.01, 0.1), "leader_flat")

    def test_exact_intersection_and_future_follower_response(self):
        leader = {0: 100.0, 1: 102.0, 2: 101.0, 3: 103.0}
        follower = {0: 50.0, 1: 50.5, 2: 51.0, 3: 52.0}
        timestamps, rows = build_observations(leader, follower, 1, 1, 0.1)
        self.assertEqual(timestamps, [0, 1, 2, 3])
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[1]["state"], "leader_down")
        self.assertAlmostEqual(rows[1]["follower_forward_return_pct"], 1.9607843137)
        self.assertEqual(rows[1]["future_ts_ms"], 3)

    def test_summary_reports_state_distributions(self):
        rows = [
            {"state": "leader_up", "leader_lookback_return_pct": 1.0,
             "follower_forward_return_pct": 0.5},
            {"state": "leader_down", "leader_lookback_return_pct": -1.0,
             "follower_forward_return_pct": -0.5},
        ]
        result = summarize(rows, 2)
        self.assertEqual(result["verdict"], "cross_asset_lead_lag_response_reported")
        self.assertEqual(result["by_state"]["leader_up"]["observations"], 1)


if __name__ == "__main__":
    unittest.main()
