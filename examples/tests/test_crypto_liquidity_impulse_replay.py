"""Deterministic tests for the ETF/stablecoin liquidity impulse replay."""

import unittest

from crypto_liquidity_impulse_replay import (
    aligned_observations,
    classify_impulse,
    summarize,
)


class LiquidityImpulseReplayTests(unittest.TestCase):
    def test_classification_keeps_two_channels_separate(self):
        self.assertEqual(
            classify_impulse(250.0, 2.0, 100.0, 1.0),
            "etf_positive_stablecoin_positive",
        )
        self.assertEqual(
            classify_impulse(-250.0, 0.2, 100.0, 1.0),
            "etf_negative_stablecoin_neutral",
        )

    def test_alignment_uses_as_of_trailing_inputs_and_future_price(self):
        flows = [
            {"date": "2026-09-01", "flow_musd": 80.0},
            {"date": "2026-09-02", "flow_musd": 140.0},
            {"date": "2026-09-03", "flow_musd": 160.0},
        ]
        stablecoins = [
            ("2026-08-30", (100.0, 1)),
            ("2026-09-01", (100.0, 2)),
            ("2026-09-02", (102.0, 3)),
            ("2026-09-03", (103.0, 4)),
        ]
        prices = [
            ("2026-09-01", (100.0, 1)),
            ("2026-09-02", (101.0, 2)),
            ("2026-09-03", (102.0, 3)),
            ("2026-09-04", (103.0, 4)),
            ("2026-09-05", (105.0, 5)),
        ]
        rows = aligned_observations(flows, stablecoins, prices, 2, 1, 100.0, 1.0, 1)
        self.assertEqual([row["date"] for row in rows], ["2026-09-02", "2026-09-03"])
        self.assertEqual(rows[0]["state"], "etf_positive_stablecoin_positive")
        self.assertAlmostEqual(rows[0]["etf_flow_musd"], 220.0)
        self.assertAlmostEqual(rows[0]["stablecoin_supply_change_pct"], 2.0)
        self.assertAlmostEqual(rows[0]["forward_return_pct"], 0.9900990099)

    def test_incomplete_inputs_are_skipped_and_short_sample_is_observe_only(self):
        rows = aligned_observations(
            [{"date": "2026-09-02", "flow_musd": 200.0}],
            [("2026-09-02", (102.0, 2))],
            [("2026-09-02", (100.0, 1)), ("2026-09-03", (101.0, 2))],
            2, 7, 100.0, 1.0, 1,
        )
        self.assertEqual(rows, [])
        result = summarize([{
            "state": "etf_positive_stablecoin_positive",
            "forward_return_pct": 1.0,
            "forward_abs_return_pct": 1.0,
        }], 2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_liquidity_impulse_windows")


if __name__ == "__main__":
    unittest.main()
