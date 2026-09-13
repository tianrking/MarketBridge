"""Deterministic tests for liquidity-stress response replay."""

import unittest

from crypto_liquidity_stress_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "response_price": {"price": price}, "state": state,
    }}


class LiquidityStressResponseReplayTests(unittest.TestCase):
    def test_stress_state_has_absolute_response(self):
        result = summarize_records([
            record(1, 100, "liquidity_stress"), record(2, 101, "normal_liquidity"),
            record(3, 102, "normal_liquidity"), record(4, 110, "normal_liquidity"),
        ], 3, 1)
        bucket = result["by_state"]["liquidity_stress"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_absolute_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "liquidity_stress_response_reported")

    def test_watch_and_normal_states_remain_separate(self):
        result = summarize_records([
            record(1, 100, "liquidity_watch"), record(2, 101, "normal_liquidity"),
            record(3, 102, "normal_liquidity"), record(4, 99, "normal_liquidity"),
        ], 3, 1)
        self.assertEqual(result["by_state"]["liquidity_watch"]["observations"], 1)
        self.assertEqual(result["by_state"]["normal_liquidity"]["observations"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_liquidity_stress")

    def test_missing_price_stays_observe_only(self):
        rows = [record(1, 100, "liquidity_stress"), record(2, 101, "normal_liquidity"),
                record(3, 102, "normal_liquidity"), record(4, 103, "normal_liquidity")]
        rows[0]["observation"]["response_price"] = None
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_liquidity_stress")


if __name__ == "__main__":
    unittest.main()
