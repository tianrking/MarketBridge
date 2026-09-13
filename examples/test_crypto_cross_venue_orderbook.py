#!/usr/bin/env python3
"""Deterministic tests for cross-venue order-book paper edges."""

import unittest

from crypto_cross_venue_orderbook_monitor import executable_vwap, summarize_books
from crypto_cross_venue_orderbook_replay import summarize_records


def book(exchange, bids, asks, ts_ms):
    return {"exchange": exchange, "symbol": "BTCUSDT", "bids": bids,
            "asks": asks, "ts_ms": ts_ms}


def record(state, edge, timestamp):
    return {
        "recorded_at_ms": timestamp,
        "observation": {"state": state, "best_opportunity": {"net_edge_bps": edge}},
    }


class CrossVenueOrderbookTests(unittest.TestCase):
    def test_vwap_requires_target_depth(self):
        vwap, qty, notional = executable_vwap([(100.0, 1.0)], 50.0)
        self.assertEqual(vwap, 100.0)
        self.assertEqual(qty, 0.5)
        self.assertEqual(notional, 50.0)
        self.assertIsNone(executable_vwap([(100.0, 0.1)], 50.0)[0])

    def test_depth_edge_uses_both_venues_and_cost_hurdle(self):
        summary = summarize_books([
            book("binance", [[99.0, 10.0]], [[100.0, 10.0]], 1000),
            book("okx", [[101.0, 10.0]], [[102.0, 10.0]], 1500),
        ], target_notional=1000.0, max_skew_ms=1000,
           paper_cost_bps=50.0, min_net_edge_bps=40.0)
        self.assertEqual(summary["state"], "cross_venue_book_edge")
        self.assertEqual(summary["best_opportunity"]["buy_exchange"], "binance")
        self.assertEqual(summary["best_opportunity"]["sell_exchange"], "okx")
        self.assertGreater(summary["best_opportunity"]["net_edge_bps"], 40.0)

    def test_stale_timestamp_skew_is_not_promoted(self):
        summary = summarize_books([
            book("binance", [[99.0, 10.0]], [[100.0, 10.0]], 1000),
            book("okx", [[101.0, 10.0]], [[102.0, 10.0]], 5000),
        ], 1000.0, max_skew_ms=100, paper_cost_bps=0, min_net_edge_bps=0)
        self.assertEqual(summary["state"], "observe_only_no_synchronized_depth")

    def test_replay_requires_consecutive_qualifying_snapshots(self):
        result = summarize_records([
            record("cross_venue_book_edge", 12.0, 1),
            record("cross_venue_book_edge", 13.0, 2),
            record("cross_venue_edge_below_paper_hurdle", 2.0, 3),
            record("cross_venue_book_edge", 11.0, 4),
        ], min_run=2, min_net_edge_bps=10.0)
        self.assertEqual(result["longest_qualifying_run"], 2)
        self.assertEqual(result["verdict"], "persistent_cross_venue_book_edge_candidate")


if __name__ == "__main__":
    unittest.main()
