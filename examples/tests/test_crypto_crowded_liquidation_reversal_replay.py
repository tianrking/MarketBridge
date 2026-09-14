import unittest

from crypto_crowded_liquidation_reversal_replay import (
    classify_state,
    liquidation_window,
    summarize,
)


class CrowdedLiquidationReversalTests(unittest.TestCase):
    def test_side_specific_flush_states_are_explicit(self):
        long_flush = {"sell_notional": 2_000_000, "buy_notional": None}
        short_flush = {"sell_notional": None, "buy_notional": 2_000_000}
        self.assertEqual(
            classify_state(0.2, -0.2, long_flush, 0.1, 0.1, 1_000_000),
            "long_crowding_flush_context",
        )
        self.assertEqual(
            classify_state(-0.2, -0.2, short_flush, 0.1, 0.1, 1_000_000),
            "short_crowding_flush_context",
        )

    def test_missing_or_non_dropping_oi_is_not_promoted(self):
        metrics = {"sell_notional": 2_000_000, "buy_notional": None}
        self.assertEqual(
            classify_state(0.2, None, metrics, 0.1, 0.1, 1_000_000),
            "observe_only_missing_alignment",
        )
        self.assertEqual(
            classify_state(0.2, 0.1, metrics, 0.1, 0.1, 1_000_000),
            "ordinary_positioning_state",
        )

    def test_liquidation_window_keeps_sides_and_bounds(self):
        events = [
            {"ts_ms": 1_000, "notional": 3.0, "side": "sell"},
            {"ts_ms": 2_000, "notional": 4.0, "side": "buy"},
            {"ts_ms": 4_000, "notional": 9.0, "side": "sell"},
        ]
        self.assertEqual(
            liquidation_window(events, 4_000, 2_000),
            {"total_notional": 9.0, "sell_notional": 9.0,
             "buy_notional": None, "event_count": 1},
        )

    def test_summary_compares_flush_with_ordinary_control(self):
        rows = [
            {"state": "long_crowding_flush_context", "forward_return_pct": 2.0},
            {"state": "short_crowding_flush_context", "forward_return_pct": -2.0},
            {"state": "ordinary_positioning_state", "forward_return_pct": 0.5},
            {"state": "ordinary_positioning_state", "forward_return_pct": 0.5},
        ]
        summary = summarize(rows, min_observations=2, min_abs_edge_bps=100.0)
        self.assertEqual(summary["verdict"], "crowded_liquidation_response_candidate")
        self.assertEqual(summary["flush_observations"], 2)


if __name__ == "__main__":
    unittest.main()
