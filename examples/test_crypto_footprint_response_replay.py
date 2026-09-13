"""Deterministic tests for footprint response replay."""

import unittest

from crypto_footprint_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "footprint": {"state": state},
    }}


class FootprintResponseReplayTests(unittest.TestCase):
    def test_bid_and_ask_pressure_are_directionally_aligned(self):
        result = summarize_records([
            record(1, 100, "footprint_bid_pressure"),
            record(2, 101, "ordinary_pressure"),
            record(3, 102, "ordinary_pressure"),
            record(4, 110, "ordinary_pressure"),
        ], 3, 1)
        bucket = result["by_state"]["bid_pressure"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_aligned_return_bps"], 1000.0)
        self.assertEqual(result["verdict"], "footprint_response_reported")

    def test_ask_pressure_uses_negative_direction(self):
        result = summarize_records([
            record(1, 100, "footprint_ask_pressure"),
            record(2, 99, "ordinary_pressure"),
            record(3, 98, "ordinary_pressure"),
            record(4, 90, "ordinary_pressure"),
        ], 3, 1)
        self.assertGreater(result["by_state"]["ask_pressure"]["mean_aligned_return_bps"], 0)

    def test_missing_prices_remain_observe_only(self):
        rows = [record(1, 100, "footprint_bid_pressure"), record(2, 101, "ordinary_pressure"),
                record(3, 102, "ordinary_pressure"), record(4, 103, "ordinary_pressure")]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_footprint_pressure")


if __name__ == "__main__":
    unittest.main()
