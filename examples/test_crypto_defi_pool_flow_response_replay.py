"""Deterministic tests for the DEX-pool response replay."""

import unittest

from crypto_defi_pool_flow_response_replay import summarize_records


def record(ts, price, states):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price},
        "pools": [{"source": "uniswap_v3", "symbol": "WETH/USDC", "state": state}
                  for state in states],
    }}


class DefiPoolResponseReplayTests(unittest.TestCase):
    def test_pressure_snapshot_reports_absolute_response(self):
        result = summarize_records([
            record(1, 100, ["high_turnover_pool"]), record(2, 101, ["normal_pool_activity"]),
            record(3, 102, ["normal_pool_activity"]), record(4, 110, ["normal_pool_activity"]),
        ], 3, 1)
        self.assertEqual(result["by_state"]["pressure"]["observations"], 1)
        self.assertAlmostEqual(result["by_state"]["pressure"]["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "defi_pool_response_reported")

    def test_ordinary_pool_snapshot_is_separate(self):
        result = summarize_records([
            record(1, 100, ["normal_pool_activity"]), record(2, 99, ["normal_pool_activity"]),
            record(3, 98, ["normal_pool_activity"]), record(4, 90, ["normal_pool_activity"]),
        ], 3, 1)
        self.assertLess(result["by_state"]["ordinary_pool_activity"]["mean_forward_return_pct"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1, 100, ["high_turnover_pool"]), record(2, 101, ["normal_pool_activity"]),
                record(3, 102, ["normal_pool_activity"]), record(4, 103, ["normal_pool_activity"])]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_pool_snapshots")


if __name__ == "__main__":
    unittest.main()
