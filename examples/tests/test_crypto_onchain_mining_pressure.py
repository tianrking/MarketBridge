"""Deterministic tests for Bitcoin mining pressure classification and replay."""

import unittest

from crypto_onchain_mining_pressure_monitor import classify, summarize
from crypto_onchain_mining_pressure_replay import summarize as summarize_replay


def data(difficulty, hashrate):
    return {
        "difficulty_change_pct": difficulty,
        "hashrate_change_7d_pct": hashrate,
        "current_hashrate_hs": 100.0,
        "current_difficulty": 200.0,
    }


def record(ts, price, mining):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "mining": mining,
    }}


class OnchainMiningPressureTests(unittest.TestCase):
    def test_classification_preserves_stress_tailwind_and_ordinary_states(self):
        self.assertEqual(classify(data(-4, 2), -3, -3, 3, 3), "miner_stress_context")
        self.assertEqual(classify(data(4, 5), -3, -3, 3, 3), "miner_tailwind_context")
        self.assertEqual(classify(data(1, 1), -3, -3, 3, 3), "ordinary_mining_context")
        self.assertEqual(classify({}, -3, -3, 3, 3), "observe_only_missing_mining_metrics")

    def test_summary_keeps_difficulty_and_hashrate_fields(self):
        result = summarize(data(-4, -5), -3, -3, 3, 3)
        self.assertEqual(result["state"], "miner_stress_context")
        self.assertEqual(result["difficulty_change_pct"], -4.0)
        self.assertEqual(result["hashrate_change_7d_pct"], -5.0)

    def test_replay_compares_miner_stress_with_ordinary_windows(self):
        rows = [
            record(1, 100, data(-4, -5)),
            record(2, 101, data(1, 1)),
            record(3, 102, data(1, 1)),
            record(4, 110, data(1, 1)),
        ]
        result = summarize_replay(rows, 3, -3, -3, 3, 3, 1)
        self.assertEqual(result["by_state"]["miner_stress_context"]["observations"], 1)
        self.assertEqual(result["verdict"], "mining_pressure_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["miner_stress_context"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_remains_observe_only_without_price_alignment(self):
        rows = [
            record(1, None, data(-4, -5)),
            record(2, 101, data(1, 1)),
            record(3, 102, data(1, 1)),
            record(4, 103, data(1, 1)),
        ]
        result = summarize_replay(rows, 3, -3, -3, 3, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_miner_stress_observations")


if __name__ == "__main__":
    unittest.main()
