#!/usr/bin/env python3
"""Deterministic tests for the on-chain transfer burst replay."""

import unittest

from crypto_onchain_transfer_burst_replay import burst_observations, summarize, transfer_rows


class OnchainTransferBurstTests(unittest.TestCase):
    def test_transfer_rows_require_positive_usd_value(self):
        rows = transfer_rows({"transfers": [
            {"ts_ms": 1, "amount_usd": 1000, "asset": "USDT"},
            {"ts_ms": 2, "amount_usd": 0, "asset": "USDT"},
            {"ts_ms": 3, "amount_usd": "bad", "asset": "USDT"},
        ]})
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["amount_usd"], 1000.0)

    def test_burst_observation_uses_future_candle_without_direction(self):
        events = [{"ts_ms": 2, "amount_usd": 600.0, "asset": "USDT", "chain": "ethereum"}]
        candles = [(1, 100.0), (2, 100.0), (3, 105.0), (4, 100.0)]
        observations = burst_observations(events, candles, 10, 1, 500.0, 2)
        self.assertEqual(len(observations), 1)
        self.assertAlmostEqual(observations[0]["forward_abs_return_pct"], 5.0)
        self.assertEqual(observations[0]["directions"], [])

    def test_summary_needs_sample_and_edge(self):
        candles = [(1, 100.0), (2, 101.0), (3, 102.0), (4, 103.0)]
        summary = summarize([], candles, 1, 1, 0.0)
        self.assertEqual(summary["verdict"], "observe_only")
        self.assertEqual(summary["burst_observations"], 0)


if __name__ == "__main__":
    unittest.main()
