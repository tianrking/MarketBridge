"""Deterministic tests for social-signal response replay."""

import unittest

from crypto_social_signal_response_replay import summarize_records


def record(price, timestamp, change):
    return {"recorded_at_ms": timestamp, "observation": {
        "social_change": change, "price": {"price": price},
    }}


class SocialSignalReplayTests(unittest.TestCase):
    def test_social_change_bucket_reports_absolute_response(self):
        result = summarize_records([
            record(100, 1, 2.0), record(102, 2, 0.0), record(105, 3, 0.0),
        ], 2, 1, 1.0)
        self.assertEqual(result["by_state"]["social_rise"]["observations"], 1)
        self.assertAlmostEqual(result["by_state"]["social_rise"]["mean_absolute_forward_return_pct"], 5.0)

    def test_small_change_is_ordinary(self):
        result = summarize_records([
            record(100, 1, 0.5), record(101, 2, 0.5), record(102, 3, 0.5),
        ], 2, 1, 1.0)
        self.assertEqual(result["by_state"]["social_rise"]["observations"], 0)
        self.assertEqual(result["by_state"]["ordinary"]["observations"], 1)

    def test_missing_price_is_not_zero(self):
        result = summarize_records([
            record(100, 1, 2.0), {"recorded_at_ms": 2, "observation": {"social_change": 2.0}},
            record(102, 3, 2.0),
        ], 1, 1, 1.0)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_social_changes")


if __name__ == "__main__":
    unittest.main()
