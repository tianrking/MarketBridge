"""Deterministic tests for the bull-call-spread response replay."""

import unittest

from crypto_options_bull_call_spread_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "spread": {"state": state, "debit": 100},
    }}


class BullCallSpreadResponseReplayTests(unittest.TestCase):
    def test_observable_spread_has_forward_response(self):
        result = summarize_records([
            record(1, 100, "bull_call_spread_quote_available"),
            record(2, 101, "spread_not_validated"), record(3, 102, "spread_not_validated"),
            record(4, 110, "spread_not_validated"),
        ], 3, 1)
        bucket = result["by_state"]["spread_observable"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "bull_call_spread_response_reported")

    def test_mark_only_is_observable_but_kept_as_same_bucket(self):
        result = summarize_records([
            record(1, 100, "bull_call_spread_mark_only"), record(2, 99, "spread_not_validated"),
            record(3, 98, "spread_not_validated"), record(4, 90, "spread_not_validated"),
        ], 3, 1)
        self.assertLess(result["by_state"]["spread_observable"]["mean_forward_return_pct"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1, 100, "bull_call_spread_quote_available"), record(2, 101, "spread_not_validated"),
                record(3, 102, "spread_not_validated"), record(4, 103, "spread_not_validated")]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_spread_snapshots")


if __name__ == "__main__":
    unittest.main()
