"""Deterministic tests for liquidation-burst response replay."""

import unittest

from crypto_liquidation_burst_response_replay import summarize_records


def record(ts, price, liquidations):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "liquidations": liquidations,
    }}


def liquidation(ts, notional, side="sell", price=100.0):
    return {"ts_ms": ts, "notional": notional, "side": side, "price": price}


class LiquidationBurstResponseReplayTests(unittest.TestCase):
    def test_burst_is_compared_with_ordinary_window(self):
        rows = [
            record(1_000, 100, [liquidation(999, 2_000_000)]),
            record(2_000, 101, []), record(3_000, 102, []), record(4_000, 110, []),
        ]
        result = summarize_records(rows, 10_000, 3, 1_000_000, 0, 1)
        self.assertEqual(result["by_state"]["liquidation_burst"]["observations"], 1)
        self.assertAlmostEqual(
            result["by_state"]["liquidation_burst"]["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "liquidation_burst_response_reported")

    def test_duplicate_events_are_not_double_counted(self):
        event = liquidation(999, 600_000)
        rows = [record(1_000, 100, [event]), record(2_000, 100, [event]),
                record(3_000, 100, []), record(4_000, 101, [])]
        result = summarize_records(rows, 10_000, 1, 1_000_000, 0, 1)
        self.assertEqual(result["unique_liquidation_events"], 1)
        self.assertEqual(result["by_state"]["liquidation_burst"]["observations"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1_000, 100, [liquidation(999, 2_000_000)]),
                record(2_000, 101, []), record(3_000, 102, []), record(4_000, 103, [])]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 10_000, 3, 1_000_000, 0, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_liquidation_bursts")


if __name__ == "__main__":
    unittest.main()
