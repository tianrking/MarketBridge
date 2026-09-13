#!/usr/bin/env python3
"""Deterministic tests for quarter-hour flow research."""

import unittest

from crypto_quarter_hour_flow_replay import (
    candle_rows,
    quarter_hour_observations,
    summarize,
    trade_rows,
)


class QuarterHourFlowTests(unittest.TestCase):
    def test_trade_rows_keep_only_positive_signed_trades(self):
        rows = trade_rows({"rows": [
            {"ts_ms": 1, "notional": 10, "side": "buy"},
            {"ts_ms": 2, "notional": 0, "side": "sell"},
            {"ts_ms": 3, "notional": 4, "side": "unknown"},
        ]})
        self.assertEqual(rows, [{"ts_ms": 1, "notional": 10.0, "side": "buy"}])

    def test_quarter_hour_signal_uses_signed_flow_and_forward_return(self):
        start = 0
        candles = [(start + index * 60_000, 100.0 + index * 0.1) for index in range(6)]
        candles = candle_rows({"candles": [
            {"open_time_ms": ts, "close": close} for ts, close in candles
        ]})
        trades = trade_rows({"rows": [
            {"ts_ms": 60_000, "notional": 80, "side": "buy"},
            {"ts_ms": 120_000, "notional": 20, "side": "sell"},
        ]})
        observations = quarter_hour_observations(candles, trades, 5, 1, 0.2, 0)
        self.assertEqual(len(observations), 1)
        self.assertEqual(observations[0]["flow_direction"], "buy")
        self.assertGreater(observations[0]["aligned_return_bps"], 0)

    def test_horizon_without_future_candle_is_excluded(self):
        candles = candle_rows({"candles": [
            {"open_time_ms": 0, "close": 100},
            {"open_time_ms": 60_000, "close": 101},
        ]})
        trades = trade_rows({"rows": [{"ts_ms": 0, "notional": 100, "side": "buy"}]})
        self.assertEqual(quarter_hour_observations(candles, trades, 5, 2, 0.2, 0), [])

    def test_missing_intermediate_candle_does_not_fake_bar_horizon(self):
        candles = candle_rows({"candles": [
            {"open_time_ms": 0, "close": 100},
            {"open_time_ms": 120_000, "close": 101},
        ]})
        trades = trade_rows({"rows": [{"ts_ms": 0, "notional": 100, "side": "buy"}]})
        self.assertEqual(quarter_hour_observations(candles, trades, 5, 1, 0.2, 0), [])

    def test_summary_applies_paper_hurdle(self):
        observations = [
            {"aligned_return_bps": 12.0, "aligned": True},
            {"aligned_return_bps": -2.0, "aligned": False},
        ]
        summary = summarize(observations, 2, 5.0, 0.0)
        self.assertEqual(summary["mean_cost_adjusted_return_bps"], 0.0)
        self.assertEqual(summary["verdict"], "quarter_hour_flow_candidate")


if __name__ == "__main__":
    unittest.main()
