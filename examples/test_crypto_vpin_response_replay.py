#!/usr/bin/env python3
"""Deterministic tests for the VPIN-proxy response replay."""

import unittest

from crypto_vpin_response_replay import (
    bucket_stats,
    trade_rows,
    volume_buckets,
    vpin_observations,
    summarize,
)


class VpinResponseTests(unittest.TestCase):
    def test_trade_rows_keep_only_complete_signed_inputs(self):
        payload = {"rows": [
            {"ts_ms": 2, "notional": 10, "price": 100, "side": "buy"},
            {"ts_ms": 1, "notional": 0, "price": 100, "side": "sell"},
            {"ts_ms": 3, "notional": 10, "price": None, "side": "sell"},
        ]}
        self.assertEqual(len(trade_rows(payload)), 1)

    def test_volume_buckets_discard_partial_tail(self):
        trades = [
            {"ts_ms": 1, "notional": 60.0, "price": 100.0, "side": "buy"},
            {"ts_ms": 2, "notional": 50.0, "price": 101.0, "side": "sell"},
            {"ts_ms": 3, "notional": 60.0, "price": 102.0, "side": "buy"},
        ]
        buckets = volume_buckets(trades, 100.0)
        self.assertEqual(len(buckets), 1)
        self.assertAlmostEqual(buckets[0]["signed_imbalance_ratio"], 10.0 / 110.0)

    def test_vpin_state_uses_only_completed_prior_window(self):
        buckets = [
            {"end_ts_ms": 1, "close_price": 100.0, "absolute_imbalance_ratio": 1.0},
            {"end_ts_ms": 2, "close_price": 102.0, "absolute_imbalance_ratio": 1.0},
            {"end_ts_ms": 3, "close_price": 99.96, "absolute_imbalance_ratio": 0.0},
            {"end_ts_ms": 4, "close_price": 101.5, "absolute_imbalance_ratio": 0.0},
            {"end_ts_ms": 5, "close_price": 102.0, "absolute_imbalance_ratio": 0.0},
        ]
        rows = vpin_observations(buckets, window_buckets=2, horizon_buckets=1, high_vpin=0.75)
        self.assertEqual([row["state"] for row in rows], ["high_vpin", "normal_vpin", "normal_vpin"])
        self.assertAlmostEqual(rows[0]["absolute_forward_return_bps"], 200.0)

    def test_summary_requires_normal_control(self):
        high = {"state": "high_vpin", "absolute_forward_return_bps": 10.0,
                "forward_return_pct": 0.1}
        result = summarize([high], min_observations=1, paper_cost_bps=0.0, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertIsNone(result["high_minus_normal_absolute_edge_bps"])

    def test_summary_reports_stress_edge_after_control(self):
        high = {"state": "high_vpin", "absolute_forward_return_bps": 10.0,
                "forward_return_pct": 0.1}
        normal = {"state": "normal_vpin", "absolute_forward_return_bps": 2.0,
                  "forward_return_pct": -0.02}
        result = summarize([high, normal], min_observations=1,
                           paper_cost_bps=1.0, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "vpin_stress_response_candidate")
        self.assertAlmostEqual(result["high_minus_normal_absolute_edge_bps"], 8.0)
        self.assertAlmostEqual(result["cost_adjusted_edge_bps"], 7.0)


if __name__ == "__main__":
    unittest.main()
