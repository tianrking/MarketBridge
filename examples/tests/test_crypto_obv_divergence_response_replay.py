"""Deterministic tests for OBV divergence response replay."""

import unittest

from crypto_obv_divergence_response_replay import (
    build_observations,
    classify_state,
    obv_values,
    summarize,
)


def bar(ts_ms, close, volume=100.0):
    return {"ts_ms": ts_ms, "open": close, "high": close + 1.0,
            "low": close - 1.0, "close": close, "volume": volume}


class ObvDivergenceResponseReplayTests(unittest.TestCase):
    def test_obv_adds_and_subtracts_close_signed_volume(self):
        rows = [bar(0, 100.0, 10.0), bar(1, 101.0, 20.0),
                bar(2, 100.0, 30.0), bar(3, 100.0, 40.0)]
        self.assertEqual(obv_values(rows), [0.0, 20.0, -10.0, -10.0])

    def test_divergence_and_confirmation_states_are_separate(self):
        self.assertEqual(classify_state(100.0, -0.20, 20.0, 0.10),
                         ("bearish_divergence", -1))
        self.assertEqual(classify_state(-100.0, 0.20, 20.0, 0.10),
                         ("bullish_divergence", 1))
        self.assertEqual(classify_state(100.0, 0.20, 20.0, 0.10),
                         ("price_up_obv_up", 1))

    def test_missing_volume_is_not_zero_filled(self):
        rows = [bar(0, 100.0), bar(1, 101.0, None), bar(2, 102.0)]
        self.assertEqual(obv_values(rows), [0.0, None, None])
        self.assertEqual(classify_state(None, None, 20.0, 0.10),
                         ("missing_volume", 0))

    def test_summary_requires_divergence_and_control(self):
        rows = [bar(index, 100.0 + (index % 3), 100.0) for index in range(10)]
        observations = build_observations(rows, 2, 1, 20.0, 0.10)
        manual = [
            {"state": "bullish_divergence", "direction_aligned_return_bps": 15.0,
             "forward_return_pct": 0.15, "forward_absolute_return_pct": 0.15},
            {"state": "price_up_obv_up", "direction_aligned_return_bps": 10.0,
             "forward_return_pct": 0.10, "forward_absolute_return_pct": 0.10},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "obv_divergence_response_reported")
        self.assertEqual(len(observations), 7)
        self.assertIn("missing_volume", result["by_state"])


if __name__ == "__main__":
    unittest.main()
