#!/usr/bin/env python3
"""Deterministic tests for event-driven trade-imbalance bar replay."""

import unittest

from crypto_trade_imbalance_bar_replay import (
    build_imbalance_bars,
    event_observations,
    summarize,
    trade_rows,
)


class TradeImbalanceBarTests(unittest.TestCase):
    def test_trade_rows_reject_missing_price_or_unknown_side(self):
        payload = {"rows": [
            {"ts_ms": 2, "notional": 10, "price": 100, "side": "buy"},
            {"ts_ms": 1, "notional": 10, "price": None, "side": "sell"},
            {"ts_ms": 3, "notional": 10, "price": 100, "side": "other"},
        ]}
        self.assertEqual(trade_rows(payload), [
            {"ts_ms": 2, "notional": 10.0, "price": 100.0, "side": "buy"}
        ])

    def test_bars_close_at_threshold_and_preserve_direction(self):
        trades = [
            {"ts_ms": 1, "notional": 60.0, "price": 100.0, "side": "buy"},
            {"ts_ms": 2, "notional": 50.0, "price": 101.0, "side": "sell"},
            {"ts_ms": 3, "notional": 60.0, "price": 102.0, "side": "buy"},
        ]
        bars = build_imbalance_bars(trades, threshold_notional=50.0, max_trades_per_bar=10)
        self.assertEqual(len(bars), 3)
        self.assertEqual([bar["direction_sign"] for bar in bars], [1, -1, 1])
        self.assertEqual(bars[0]["close_reason"], "imbalance_threshold")

    def test_event_observation_aligns_strong_buy_and_sell(self):
        bars = [
            {"end_ts_ms": 1, "close_price": 100.0, "signed_notional": 100.0,
             "total_notional": 100.0, "trade_count": 1, "imbalance_ratio": 1.0,
             "direction_sign": 1, "close_reason": "imbalance_threshold"},
            {"end_ts_ms": 2, "close_price": 102.0, "signed_notional": -100.0,
             "total_notional": 100.0, "trade_count": 1, "imbalance_ratio": -1.0,
             "direction_sign": -1, "close_reason": "imbalance_threshold"},
            {"end_ts_ms": 3, "close_price": 99.96, "signed_notional": 0.0,
             "total_notional": 100.0, "trade_count": 2, "imbalance_ratio": 0.0,
             "direction_sign": 0, "close_reason": "trade_count_cap"},
        ]
        rows = event_observations(bars, horizon_bars=1, min_imbalance_ratio=0.5)
        self.assertEqual([row["state"] for row in rows], ["strong_buy", "strong_sell"])
        self.assertAlmostEqual(rows[0]["aligned_return_bps"], 200.0)
        self.assertAlmostEqual(rows[1]["aligned_return_bps"], 200.0)

    def test_summary_keeps_insufficient_control_observe_only(self):
        row = {"state": "strong_buy", "forward_return_pct": 1.0,
               "aligned_return_bps": 100.0}
        result = summarize([row], min_observations=1, paper_cost_bps=0.0, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertIsNone(result["aligned_minus_balanced_absolute_edge_bps"])

    def test_summary_reports_control_adjusted_edge(self):
        strong = {"state": "strong_sell", "forward_return_pct": -0.02,
                  "aligned_return_bps": 2.0}
        balanced = {"state": "balanced", "forward_return_pct": 0.01,
                    "aligned_return_bps": None}
        result = summarize([strong, balanced], min_observations=1,
                           paper_cost_bps=0.5, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "trade_imbalance_bar_response_candidate")
        self.assertAlmostEqual(result["aligned_minus_balanced_absolute_edge_bps"], 1.0)
        self.assertAlmostEqual(result["cost_adjusted_edge_bps"], 0.5)


if __name__ == "__main__":
    unittest.main()
