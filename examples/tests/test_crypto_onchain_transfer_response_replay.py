"""Deterministic tests for the frozen on-chain transfer response replay."""

import unittest

from crypto_onchain_transfer_response_replay import summarize_records


def record(ts, price, transfers):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "transfers": transfers,
    }}


def transfer(ts, amount, asset="USDT"):
    return {"ts_ms": ts, "amount_usd": amount, "asset": asset,
            "chain": "ethereum", "source": "whale_alert", "direction": "unknown"}


class OnchainTransferResponseReplayTests(unittest.TestCase):
    def test_burst_is_compared_with_ordinary_window(self):
        rows = [
            record(1_000, 100, [transfer(999, 2_000_000)]),
            record(2_000, 101, []), record(3_000, 102, []), record(4_000, 110, []),
        ]
        result = summarize_records(rows, 10_000, 3, 1_000_000, 0, 1)
        self.assertEqual(result["by_state"]["transfer_burst"]["observations"], 1)
        self.assertAlmostEqual(result["by_state"]["transfer_burst"]["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "onchain_transfer_response_reported")

    def test_duplicate_transfer_snapshots_are_not_double_counted(self):
        event = transfer(999, 600_000)
        rows = [record(1_000, 100, [event]), record(2_000, 100, [event]),
                record(3_000, 100, []), record(4_000, 101, [])]
        result = summarize_records(rows, 10_000, 1, 1_000_000, 0, 1)
        self.assertEqual(result["unique_transfer_events"], 1)
        self.assertEqual(result["by_state"]["transfer_burst"]["observations"], 0)

    def test_missing_price_remains_observe_only(self):
        rows = [record(1_000, 100, [transfer(999, 2_000_000)]),
                record(2_000, 101, []), record(3_000, 102, []), record(4_000, 103, [])]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 10_000, 3, 1_000_000, 0, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_transfer_bursts")


if __name__ == "__main__":
    unittest.main()
