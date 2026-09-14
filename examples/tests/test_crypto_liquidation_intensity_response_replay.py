"""Deterministic tests for liquidation-intensity response replay."""

import unittest

from crypto_liquidation_intensity_response_replay import (
    build_observations,
    classify_intensity,
    intensity_at,
    summarize,
)


def bar(ts_ms, close, volume=10.0):
    return {"ts_ms": ts_ms, "open": close, "high": close + 1.0,
            "low": close - 1.0, "close": close, "volume": volume}


class LiquidationIntensityResponseReplayTests(unittest.TestCase):
    def setUp(self):
        self.candles = [bar(index * 60_000, 100.0 + index, 10.0) for index in range(8)]
        self.events = [
            {"ts_ms": 2 * 60_000 + 1, "notional": 50.0, "side": "sell"},
            {"ts_ms": 3 * 60_000 + 1, "notional": 50.0, "side": "buy"},
        ]

    def test_intensity_uses_typical_price_times_base_volume(self):
        metrics = intensity_at(self.candles, self.events, 3, 2)
        self.assertEqual(metrics["status"], "available")
        self.assertAlmostEqual(metrics["liquidation_notional"], 100.0)
        self.assertAlmostEqual(metrics["quote_volume"], ((102.0) + (103.0)) * 10.0, places=6)
        self.assertAlmostEqual(metrics["sell_share"], 0.5)

    def test_high_and_ordinary_states_are_explicit(self):
        self.assertEqual(classify_intensity({"status": "available", "liquidation_notional": 100.0,
                                             "intensity_ratio": 0.20}, 0.10, 0.0),
                         "high_liquidation_intensity")
        self.assertEqual(classify_intensity({"status": "available", "liquidation_notional": 1.0,
                                             "intensity_ratio": 0.01}, 0.10, 0.0),
                         "ordinary_liquidation_intensity")
        self.assertEqual(classify_intensity({"status": "missing_quote_volume"}, 0.10, 0.0),
                         "missing_quote_volume")

    def test_build_observations_keeps_forward_window_and_state(self):
        observations = build_observations(self.candles, self.events, 2, 1, 0.01, 0.0)
        self.assertEqual(len(observations), 6)
        self.assertIn("forward_abs_return_pct", observations[0])
        self.assertEqual(observations[2]["state"], "high_liquidation_intensity")

    def test_summary_requires_high_and_ordinary_controls(self):
        observations = [
            {"state": "high_liquidation_intensity", "forward_return_pct": 1.0,
             "forward_abs_return_pct": 1.0,
             "metrics": {"intensity_ratio": 0.2}},
            {"state": "ordinary_liquidation_intensity", "forward_return_pct": 0.2,
             "forward_abs_return_pct": 0.2,
             "metrics": {"intensity_ratio": 0.01}},
        ]
        result = summarize(observations, 1, 10.0)
        self.assertEqual(result["absolute_move_edge_bps_high_minus_ordinary"], 80.0)
        self.assertEqual(result["verdict"], "liquidation_intensity_response_reported")


if __name__ == "__main__":
    unittest.main()
