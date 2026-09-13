"""Deterministic tests for option term-structure response replay."""

import unittest

from crypto_options_term_structure_response_replay import summarize_records


def record(ts, price, slope):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "term_structure": {"atm_iv_slope_iv": slope},
    }}


class OptionsTermStructureResponseReplayTests(unittest.TestCase):
    def test_upward_state_has_forward_response(self):
        result = summarize_records([
            record(1, 100, 4), record(2, 101, 0), record(3, 102, 0), record(4, 110, 0),
        ], 3, 3, 1)
        bucket = result["by_state"]["upward_iv_term_structure"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "options_term_structure_response_reported")

    def test_inverted_state_is_separate(self):
        result = summarize_records([
            record(1, 100, -4), record(2, 99, 0), record(3, 98, 0), record(4, 90, 0),
        ], 3, 3, 1)
        self.assertLess(result["by_state"]["inverted_iv_term_structure"]["mean_forward_return_pct"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1, 100, 4), record(2, 101, 0), record(3, 102, 0), record(4, 103, 0)]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_term_structure_windows")


if __name__ == "__main__":
    unittest.main()
