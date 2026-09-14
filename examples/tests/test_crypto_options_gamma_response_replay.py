"""Deterministic tests for the option-gamma response replay."""

import unittest

from crypto_options_gamma_response_replay import summarize_records


def record(ts, price, near_share=0.7, concentration=0.2):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price},
        "gamma": {"target_expiry": {
            "near_spot_share": near_share,
            "concentration_at_strike": concentration,
        }},
    }}


class GammaResponseReplayTests(unittest.TestCase):
    def test_concentrated_bucket_reports_forward_and_absolute_response(self):
        result = summarize_records([record(1, 100), record(2, 101), record(3, 102),
                                    record(4, 110)], 3, 0.5, 0.1, 1)
        bucket = result["by_state"]["near_spot_gamma_concentration"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "gamma_concentration_response_reported")

    def test_thresholds_put_snapshot_in_other_bucket(self):
        result = summarize_records([record(1, 100, 0.2), record(2, 101), record(3, 102),
                                    record(4, 103)], 3, 0.5, 0.1, 1)
        self.assertEqual(result["by_state"]["near_spot_gamma_concentration"]["observations"], 0)
        self.assertEqual(result["by_state"]["other_gamma_state"]["observations"], 1)

    def test_missing_price_is_excluded(self):
        missing = record(1, 100)
        missing["observation"]["price"] = None
        result = summarize_records([missing, record(2, 101), record(3, 102), record(4, 103)],
                                   3, 0.5, 0.1, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_gamma_observations")


if __name__ == "__main__":
    unittest.main()
