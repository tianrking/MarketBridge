"""Deterministic tests for options-skew response replay."""

import unittest

from crypto_options_skew_response_replay import summarize_records


def record(ts, price, skew):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price},
        "target_expiry": {"put_call_skew_iv": skew},
        "term_structure": {"state": "flat_iv_term_structure"},
    }}


class OptionsSkewResponseReplayTests(unittest.TestCase):
    def test_downside_protection_state_has_forward_response(self):
        result = summarize_records([
            record(1, 100, 8), record(2, 101, 0), record(3, 102, 0), record(4, 110, 0),
        ], 3, 3, 1)
        bucket = result["by_state"]["downside_protection_demand"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "options_skew_response_reported")

    def test_upside_and_balanced_states_are_separate(self):
        result = summarize_records([
            record(1, 100, -8), record(2, 99, 0), record(3, 98, 0), record(4, 90, 0),
        ], 3, 3, 1)
        self.assertLess(result["by_state"]["upside_call_demand"]["mean_forward_return_pct"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1, 100, 8), record(2, 101, 0), record(3, 102, 0), record(4, 103, 0)]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_skew_snapshots")


if __name__ == "__main__":
    unittest.main()
