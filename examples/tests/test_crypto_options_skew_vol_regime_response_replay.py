"""Deterministic tests for low-volatility plus downside-skew replay."""

import unittest

from crypto_options_skew_vol_regime_response_replay import (
    realized_volatility_proxy_pct,
    skew_vol_state,
    summarize_records,
)


class OptionsSkewVolRegimeResponseReplayTests(unittest.TestCase):
    def test_unannualized_volatility_proxy_and_joint_state(self):
        self.assertIsNotNone(realized_volatility_proxy_pct([100.0, 100.1, 100.0]))
        self.assertEqual(skew_vol_state(0.2, 5.0, 1.0, 3.0),
                         "low_vol_downside_skew")
        self.assertEqual(skew_vol_state(2.0, 5.0, 1.0, 3.0),
                         "downside_skew_without_low_vol")

    def test_replay_requires_trailing_window_and_reports_joint_state(self):
        prices = [100.0, 100.1, 100.0, 100.1, 99.9, 100.0, 100.2, 99.8]
        records = [{
            "recorded_at_ms": index,
            "observation": {
                "price": {"price": price},
                "target_expiry": {"put_call_skew_iv": 5.0},
            },
        } for index, price in enumerate(prices)]
        result = summarize_records(records, 1, 3, 1.0, 3.0, 1)
        self.assertEqual(result["low_vol_downside_skew_windows"], 5)
        self.assertEqual(result["verdict"],
                         "options_low_vol_downside_skew_response_reported")


if __name__ == "__main__":
    unittest.main()
