#!/usr/bin/env python3
"""Deterministic tests for the crypto volatility breakout replay."""

import unittest

from crypto_volatility_breakout_replay import (
    breakout_features,
    classify_event,
    flow_ratio_at,
    forward_return,
    realized_vol_pct,
    summarize,
)


def bar(ts_ms, close, high=None, low=None, volume=100.0):
    return {"ts_ms": ts_ms, "open": close, "close": close,
            "high": high if high is not None else close,
            "low": low if low is not None else close, "volume": volume}


class VolatilityBreakoutTests(unittest.TestCase):
    def test_realized_vol_is_zero_for_flat_series(self):
        self.assertEqual(realized_vol_pct([100.0, 100.0, 100.0]), 0.0)

    def test_breakout_features_detect_compressed_up_break(self):
        rows = [bar(index, 100.0 + (index % 2) * 0.8, high=101.0, low=99.0, volume=100.0)
                for index in range(15)]
        rows.extend(bar(index, 100.0, high=100.2, low=99.8, volume=100.0) for index in range(15, 20))
        rows[-1] = bar(19, 101.0, high=101.2, low=100.8, volume=200.0)
        features = breakout_features(rows, 19, 4, 4, 8, 0.75, 0.0, 1.2)
        self.assertEqual(features["direction"], 1)
        self.assertTrue(features["compressed"])
        self.assertTrue(features["volume_confirmed"])

    def test_flow_conflict_and_forward_return_are_explicit(self):
        rows = [bar(index, 100.0) for index in range(8)]
        rows[3] = {"ts_ms": 3, "side": "sell", "notional": 80.0}
        rows[4] = {"ts_ms": 4, "side": "buy", "notional": 20.0}
        flow = flow_ratio_at(rows[3:5], 3, 10)
        self.assertAlmostEqual(flow["ratio"], -0.6)
        self.assertEqual(classify_event({"direction": 1, "compressed": True,
                                         "volume_confirmed": True}, flow, 0.2),
                         "breakout_flow_conflict")
        self.assertAlmostEqual(forward_return([bar(i, 100.0 + i) for i in range(8)], 2, 3), 100.0 * (105.0 / 102.0 - 1.0))

    def test_summary_uses_numeric_direction_sign(self):
        result = summarize([{"direction": "up", "direction_sign": 1,
                             "forward_return_pct": 2.0,
                             "classification": "breakout_confirmed"}])
        self.assertEqual(result["directional_hit_rate"], 1.0)
        self.assertEqual(result["mean_aligned_return_pct"], 2.0)

    def test_summary_applies_paper_cost_hurdle(self):
        result = summarize([{"direction_sign": 1, "forward_return_pct": 2.0,
                             "classification": "breakout_confirmed"}],
                           roundtrip_cost_bps=250.0, min_cost_adjusted_edge_bps=0.0,
                           min_observations=1)
        self.assertAlmostEqual(result["mean_cost_adjusted_aligned_return_pct"], -0.5)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
