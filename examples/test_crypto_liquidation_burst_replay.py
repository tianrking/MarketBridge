#!/usr/bin/env python3
"""Deterministic tests for rolling liquidation-burst replay."""

import unittest

from crypto_liquidation_burst_replay import (
    burst_observations,
    liquidation_rows,
    summarize,
)


class LiquidationBurstReplayTests(unittest.TestCase):
    def test_liquidation_rows_keep_positive_events_and_side_metadata(self):
        rows = liquidation_rows({"rows": [
            {"ts_ms": 1, "notional": 100.0, "side": "sell"},
            {"ts_ms": 2, "notional": 0.0, "side": "buy"},
            {"ts_ms": 3, "notional": 50.0, "side": "buy"},
        ]})
        self.assertEqual(rows, [
            {"ts_ms": 1, "notional": 100.0, "side": "sell"},
            {"ts_ms": 3, "notional": 50.0, "side": "buy"},
        ])

    def test_burst_trigger_is_cooled_down_and_measures_absolute_move(self):
        events = [
            {"ts_ms": 1, "notional": 60.0, "side": "sell"},
            {"ts_ms": 2, "notional": 60.0, "side": "buy"},
            {"ts_ms": 3, "notional": 60.0, "side": "sell"},
        ]
        candles = [(index, value) for index, value in enumerate(
            [100.0, 100.0, 100.0, 105.0, 95.0, 100.0, 100.0]
        )]
        observations = burst_observations(events, candles, 3, 1, 100.0, 2)
        self.assertEqual(len(observations), 2)
        self.assertGreater(observations[0]["forward_abs_return_pct"], 0.0)
        self.assertEqual(observations[0]["sell_notional"], 60.0)

    def test_summary_stays_observe_only_without_threshold_events(self):
        candles = [(index, 100.0 + index) for index in range(6)]
        result = summarize([], candles, 1, 1, 0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertEqual(result["burst_observations"], 0)


if __name__ == "__main__":
    unittest.main()
