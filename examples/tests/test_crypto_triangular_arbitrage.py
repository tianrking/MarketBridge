#!/usr/bin/env python3
"""Deterministic tests for triangular quote consistency research."""

import unittest

from crypto_triangular_arbitrage_monitor import summarize_quotes
from crypto_triangular_arbitrage_replay import summarize_records


def quote(symbol, bid, ask, ts_ms=1000):
    return {
        "symbol": symbol, "exchange": "binance",
        "payload": {"bid": bid, "ask": ask},
        "freshness": {"ts_source": ts_ms},
    }


def record(state, edge, timestamp):
    return {
        "recorded_at_ms": timestamp,
        "observation": {"state": state, "best_path": {"net_edge_bps": edge}},
    }


class TriangularArbitrageTests(unittest.TestCase):
    def test_positive_cycle_uses_ask_then_bid_directions(self):
        result = summarize_quotes([
            quote("BTCUSDT", 99.0, 100.0),
            quote("ETHBTC", 0.049, 0.05),
            quote("ETHUSDT", 5.2, 5.3),
        ], "binance", 1000.0, paper_cost_bps=10.0,
           max_skew_ms=10, min_net_edge_bps=0.0)
        self.assertEqual(result["state"], "triangular_quote_edge")
        self.assertEqual(result["best_path"]["path"], "USDT->BTC->ETH->USDT")
        self.assertGreater(result["best_path"]["gross_edge_bps"], 0)

    def test_timestamp_skew_blocks_cycle(self):
        result = summarize_quotes([
            quote("BTCUSDT", 99.0, 100.0, 1000),
            quote("ETHBTC", 0.049, 0.05, 2000),
            quote("ETHUSDT", 5.2, 5.3, 1000),
        ], "binance", 1000.0, 0, max_skew_ms=100, min_net_edge_bps=0)
        self.assertEqual(result["state"], "observe_only_missing_synchronized_triangle")

    def test_replay_requires_consecutive_edges(self):
        result = summarize_records([
            record("triangular_quote_edge", 12.0, 1),
            record("triangular_quote_edge", 13.0, 2),
            record("triangular_edge_below_paper_hurdle", 1.0, 3),
        ], min_run=2, min_net_edge_bps=5.0)
        self.assertEqual(result["longest_qualifying_run"], 2)
        self.assertEqual(result["verdict"], "persistent_triangular_quote_edge_candidate")


if __name__ == "__main__":
    unittest.main()
