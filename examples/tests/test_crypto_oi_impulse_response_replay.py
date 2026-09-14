"""Deterministic tests for the OI impulse response replay."""

import unittest

from crypto_oi_impulse_response_replay import summarize_records


def record(price, timestamp, oi_change):
    return {"recorded_at_ms": timestamp, "observation": {
        "oi_change_pct": oi_change, "price": {"price": price},
    }}


class OIImpulseReplayTests(unittest.TestCase):
    def test_expansion_bucket_reports_absolute_response(self):
        result = summarize_records([
            record(100, 1, 1.0), record(102, 2, 0.0), record(105, 3, 0.0),
        ], 2, 1, 0.25)
        bucket = result["by_state"]["oi_expansion"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_absolute_forward_return_pct"], 5.0)

    def test_threshold_keeps_small_changes_ordinary(self):
        result = summarize_records([
            record(100, 1, 0.2), record(101, 2, 0.2), record(102, 3, 0.2),
        ], 2, 1, 0.25)
        self.assertEqual(result["by_state"]["oi_expansion"]["observations"], 0)
        self.assertEqual(result["by_state"]["ordinary"]["observations"], 1)

    def test_missing_price_is_not_zero(self):
        result = summarize_records([
            record(100, 1, 1.0), {"recorded_at_ms": 2, "observation": {"oi_change_pct": 1.0}},
            record(102, 3, 1.0),
        ], 1, 1, 0.25)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_oi_impulses")


if __name__ == "__main__":
    unittest.main()
