"""Deterministic tests for liquidation price-cluster response replay."""

import unittest

from crypto_liquidation_price_cluster_response_replay import summarize_records


def record(ts, price, liquidations):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "liquidations": liquidations,
    }}


def liquidation(ts, price, notional, side="sell"):
    return {"ts_ms": ts, "price": price, "notional": notional, "side": side}


class LiquidationPriceClusterResponseReplayTests(unittest.TestCase):
    def test_cluster_response_is_compared_with_ordinary(self):
        rows = [
            record(1_000, 100, [liquidation(999, 100, 2_000_000), liquidation(999, 100.1, 1_000_000)]),
            record(2_000, 101, []), record(3_000, 102, []), record(4_000, 110, []),
        ]
        result = summarize_records(rows, 10_000, 3, 1_000_000, 25, 0.5, 0, 1)
        bucket = result["by_state"]["liquidation_price_cluster"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "liquidation_price_cluster_response_reported")

    def test_duplicate_events_are_not_double_counted(self):
        event = liquidation(999, 100, 600_000)
        rows = [record(1_000, 100, [event]), record(2_000, 100, [event]),
                record(3_000, 100, []), record(4_000, 101, [])]
        result = summarize_records(rows, 10_000, 1, 1_000_000, 25, 0.5, 0, 1)
        self.assertEqual(result["unique_liquidation_events"], 1)
        self.assertEqual(result["by_state"]["liquidation_price_cluster"]["observations"], 0)

    def test_missing_price_stays_observe_only(self):
        rows = [record(1_000, 100, [liquidation(999, 100, 2_000_000)]),
                record(2_000, 101, []), record(3_000, 102, []), record(4_000, 103, [])]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 10_000, 3, 1_000_000, 25, 0.5, 0, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_price_clusters")


if __name__ == "__main__":
    unittest.main()
