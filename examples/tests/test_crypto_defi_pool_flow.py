#!/usr/bin/env python3
"""Deterministic tests for the DeFi pool flow case."""

import unittest

from crypto_defi_pool_flow_monitor import classify_pool, summarize_signals
from crypto_defi_pool_flow_replay import summarize_records


class DefiPoolFlowTests(unittest.TestCase):
    def test_signal_metrics_group_by_source_and_symbol(self):
        grouped = summarize_signals([
            {"source": "uniswap_v3", "symbol": "ETHUSDC", "metric": "pool_liquidity_usd", "value": 1000},
            {"source": "uniswap_v3", "symbol": "ETHUSDC", "metric": "swap_volume_h1", "value": 300},
            {"source": "other", "symbol": "ETHUSDC", "metric": "swap_volume_h1", "value": 1},
        ])
        self.assertEqual(grouped[("uniswap_v3", "ETHUSDC")]["swap_volume_h1"], 300.0)
        self.assertEqual(len(grouped), 2)

    def test_thin_high_flow_state_requires_both_dimensions(self):
        self.assertEqual(classify_pool({"pool_liquidity_usd": 1000, "swap_volume_h1": 300}, 2000, 0.25),
                         "thin_liquidity_high_flow")
        self.assertEqual(classify_pool({"pool_liquidity_usd": 10000, "swap_volume_h1": 300}, 2000, 0.25),
                         "normal_pool_activity")
        self.assertEqual(classify_pool({"pool_liquidity_usd": 1000}, 2000, 0.25),
                         "observe_only_missing_liquidity_or_volume")

    def test_replay_requires_consecutive_pressure(self):
        records = [
            {"recorded_at_ms": 1, "observation": {"pools": [{"source": "u", "symbol": "A", "state": "high_turnover_pool"}]}},
            {"recorded_at_ms": 2, "observation": {"pools": [{"source": "u", "symbol": "A", "state": "high_turnover_pool"}]}},
            {"recorded_at_ms": 3, "observation": {"pools": [{"source": "u", "symbol": "A", "state": "normal_pool_activity"}]}},
        ]
        result = summarize_records(records, 2)
        self.assertEqual(result["by_pool"]["u:A"]["longest_stress_run"], 2)
        self.assertEqual(result["by_pool"]["u:A"]["verdict"], "persistent_defi_pool_pressure_candidate")


if __name__ == "__main__":
    unittest.main()
