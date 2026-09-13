"""Deterministic tests for spot/perp depth-gap response replay."""

import unittest

from crypto_spot_perp_depth_gap_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "response_price": {"price": price}, "state": state,
    }}


class SpotPerpDepthGapResponseReplayTests(unittest.TestCase):
    def test_perp_advantage_has_forward_response(self):
        result = summarize_records([
            record(1, 100, "perp_depth_advantage_observation"),
            record(2, 101, "no_material_depth_gap"), record(3, 102, "no_material_depth_gap"),
            record(4, 110, "no_material_depth_gap"),
        ], 3, 1)
        bucket = result["by_state"]["perp_depth_advantage_observation"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "spot_perp_depth_gap_response_reported")

    def test_spot_and_perp_advantage_states_remain_separate(self):
        result = summarize_records([
            record(1, 100, "spot_depth_advantage_observation"),
            record(2, 101, "no_material_depth_gap"), record(3, 102, "no_material_depth_gap"),
            record(4, 99, "no_material_depth_gap"),
        ], 3, 1)
        self.assertEqual(result["by_state"]["spot_depth_advantage_observation"]["observations"], 1)
        self.assertEqual(result["by_state"]["perp_depth_advantage_observation"]["observations"], 0)

    def test_missing_price_stays_observe_only(self):
        rows = [record(1, 100, "perp_depth_advantage_observation"),
                record(2, 101, "no_material_depth_gap"), record(3, 102, "no_material_depth_gap"),
                record(4, 103, "no_material_depth_gap")]
        rows[0]["observation"]["response_price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_depth_gap_states")


if __name__ == "__main__":
    unittest.main()
