"""Deterministic tests for Bitcoin mempool pressure classification and replay."""

import unittest

from crypto_onchain_mempool_pressure_monitor import classify, summarize
from crypto_onchain_mempool_pressure_replay import summarize as summarize_replay


def data(fastest, vsize_mb):
    return {
        "mempool_vsize_mb": vsize_mb,
        "fee_rates_sat_vb": {"fastest": fastest},
        "tip_height": 900000,
    }


def record(ts, price, mempool):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "mempool": mempool,
    }}


class OnchainMempoolPressureTests(unittest.TestCase):
    def test_classification_preserves_high_low_and_ordinary_states(self):
        self.assertEqual(classify(data(25, 30), 20, 3, 150, 25), "high_fee_pressure")
        self.assertEqual(classify(data(2, 20), 20, 3, 150, 25), "low_fee_pressure")
        self.assertEqual(classify(data(8, 80), 20, 3, 150, 25), "ordinary_fee_pressure")
        self.assertEqual(classify({}, 20, 3, 150, 25), "observe_only_missing_mempool_metrics")

    def test_summary_exposes_provider_fields_and_state(self):
        result = summarize(data(25, 30), 20, 3, 150, 25)
        self.assertEqual(result["state"], "high_fee_pressure")
        self.assertEqual(result["fastest_fee_sat_vb"], 25.0)
        self.assertEqual(result["mempool_vsize_mb"], 30.0)

    def test_replay_compares_high_pressure_with_ordinary_windows(self):
        rows = [
            record(1, 100, data(25, 30)),
            record(2, 101, data(8, 80)),
            record(3, 102, data(8, 80)),
            record(4, 110, data(8, 80)),
        ]
        result = summarize_replay(rows, 3, 20, 3, 150, 25, 1)
        self.assertEqual(result["by_state"]["high_fee_pressure"]["observations"], 1)
        self.assertEqual(result["verdict"], "mempool_pressure_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["high_fee_pressure"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_remains_observe_only_without_price_alignment(self):
        rows = [
            record(1, None, data(25, 30)),
            record(2, 101, data(8, 80)),
            record(3, 102, data(8, 80)),
            record(4, 103, data(8, 80)),
        ]
        result = summarize_replay(rows, 3, 20, 3, 150, 25, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_high_pressure_observations")


if __name__ == "__main__":
    unittest.main()
