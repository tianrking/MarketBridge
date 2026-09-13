"""Deterministic tests for order-book imbalance response replay."""

import unittest

from crypto_microstructure_response_replay import summarize_records


def record(ts, price, signal):
    return {"recorded_at_ms": ts, "observation": {
        "response_price": {"price": price}, "signal": signal,
    }}


class MicrostructureResponseReplayTests(unittest.TestCase):
    def test_bid_pressure_response_is_aligned(self):
        result = summarize_records([
            record(1, 100, "bid_pressure_candidate"),
            record(2, 101, "balanced_book"), record(3, 102, "balanced_book"),
            record(4, 110, "balanced_book"),
        ], 3, 1)
        bucket = result["by_state"]["bid_pressure_candidate"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_aligned_return_bps"], 1000.0)
        self.assertEqual(result["verdict"], "microstructure_response_reported")

    def test_funding_conflict_states_are_separate(self):
        result = summarize_records([
            record(1, 100, "ask_pressure_with_short_crowding_conflict"),
            record(2, 101, "balanced_book"), record(3, 102, "balanced_book"),
            record(4, 90, "balanced_book"),
        ], 3, 1)
        bucket = result["by_state"]["ask_pressure_with_short_crowding_conflict"]
        self.assertEqual(bucket["observations"], 1)
        self.assertGreater(bucket["mean_aligned_return_bps"], 0)

    def test_missing_price_stays_observe_only(self):
        rows = [record(1, 100, "bid_pressure_candidate"),
                record(2, 101, "balanced_book"), record(3, 102, "balanced_book"),
                record(4, 103, "balanced_book")]
        rows[0]["observation"]["response_price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_pressure_states")


if __name__ == "__main__":
    unittest.main()
