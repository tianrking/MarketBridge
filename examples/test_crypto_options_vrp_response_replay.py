"""Deterministic tests for option IV-minus-RV response replay."""

import unittest

from crypto_options_vrp_response_replay import summarize_records


def record(ts, vrp, spot, expiry="2030-01-01"):
    return {"recorded_at_ms": ts, "observation": {
        "target_expiry": {"expiry_time": expiry},
        "vrp": {"iv_minus_rv_iv_points": vrp},
        "realized_volatility": {"spot_close": spot},
    }}


class OptionsVrpResponseReplayTests(unittest.TestCase):
    def test_premium_regime_has_forward_response(self):
        result = summarize_records([
            record(1, 6.0, 100), record(2, 0.0, 101),
            record(3, 0.0, 102), record(4, 0.0, 110),
        ], 5.0, 3, 1)
        bucket = result["by_state"]["implied_volatility_premium"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "options_vrp_response_reported")

    def test_realized_above_implied_is_separate_state(self):
        result = summarize_records([
            record(1, -6.0, 100), record(2, 99.0, 101),
            record(3, 99.0, 102), record(4, 99.0, 90),
        ], 5.0, 3, 1)
        bucket = result["by_state"]["realized_volatility_above_implied"]
        self.assertEqual(bucket["observations"], 1)
        self.assertLess(bucket["mean_forward_return_pct"], 0)

    def test_missing_spot_stays_out_of_aligned_sample(self):
        rows = [record(1, 6.0, 100), record(2, 0.0, 101),
                record(3, 0.0, 102), record(4, 0.0, 103)]
        rows[0]["observation"]["realized_volatility"]["spot_close"] = None
        result = summarize_records(rows, 5.0, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_vrp_regimes")


if __name__ == "__main__":
    unittest.main()
