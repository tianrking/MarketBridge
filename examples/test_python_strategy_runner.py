#!/usr/bin/env python3
"""Deterministic tests for Python-first strategy dispatch."""

import unittest
from types import SimpleNamespace

from python_strategy_runner import (
    score_cross_asset_momentum,
    score_funding_convergence,
    score_options_skew,
    score_options_vrp,
    score_volatility_breakout,
)


def args():
    return SimpleNamespace(
        expiry_days=30.0, atm_band=0.03, wing_min=0.85, wing_max=1.15,
        min_skew_iv=3.0, min_term_slope_iv=3.0, rv_interval="1h",
        rv_bars=2, vrp_threshold=5.0,
        breakout_horizon_bars=6, range_bars=2, compression_window=2,
        baseline_window=4, max_compression_ratio=0.75, breakout_buffer=0.0,
        volume_multiplier=1.2,
        min_spread_bps_per_hour=0.5,
        symbol="BTCUSDT",
    )


def options_payload():
    expiry = "2099-01-01T00:00:00Z"
    return {"chains": [
        {"payload": {"expiry_time": expiry, "strike": 100.0, "mark_iv": 50.0,
                      "option_type": "call", "underlying_price": 100.0}},
        {"payload": {"expiry_time": expiry, "strike": 95.0, "mark_iv": 56.0,
                      "option_type": "put", "underlying_price": 100.0}},
        {"payload": {"expiry_time": expiry, "strike": 105.0, "mark_iv": 50.0,
                      "option_type": "call", "underlying_price": 100.0}},
    ]}


class PythonStrategyRunnerTests(unittest.TestCase):
    def test_cross_asset_momentum_dispatch_reports_gross_evidence(self):
        settings = args()
        settings.cross_asset_lookback = 2
        settings.cross_asset_horizon = 1
        settings.cross_asset_top_k = 1
        settings.cross_asset_min_edge_bps = 0.0
        settings.cross_asset_min_observations = 3
        candles = {
            "BTCUSDT": {"candles": [{"open_time_ms": index, "close": value}
                                     for index, value in enumerate([100, 101, 102, 104, 106, 108, 110])]},
            "ETHUSDT": {"candles": [{"open_time_ms": index, "close": 100.0}
                                     for index in range(7)]},
            "SOLUSDT": {"candles": [{"open_time_ms": index, "close": value}
                                     for index, value in enumerate([100, 99, 98, 97, 96, 95, 94])]},
        }
        score, maximum, verdict, evidence = score_cross_asset_momentum(
            {"cross_asset_klines": candles}, settings, {}
        )
        self.assertEqual((score, maximum), (1, 1))
        self.assertEqual(verdict, "cross-asset momentum observation")
        self.assertTrue(any("mean top-basket edge" in item for item in evidence))

    def test_options_skew_dispatch_keeps_evidence(self):
        score, maximum, verdict, evidence = score_options_skew(
            {"options": options_payload()}, args(), {}
        )
        self.assertEqual((score, maximum), (1, 2))
        self.assertEqual(verdict, "options skew observation")
        self.assertTrue(any("put-call skew" in item for item in evidence))

    def test_options_vrp_dispatch_is_read_only(self):
        data = {"options": options_payload(), "rv_klines": {"candles": [
            {"close": 100.0}, {"close": 101.0}, {"close": 100.0},
        ]}}
        score, maximum, verdict, evidence = score_options_vrp(data, args(), {})
        self.assertEqual(maximum, 1)
        self.assertIn(score, (0, 1))
        self.assertIn(verdict, ("implied volatility premium observation", "observe only"))
        self.assertTrue(any("ATM IV" in item for item in evidence) or evidence == ["missing option ATM IV or realized-volatility window"])

    def test_volatility_breakout_dispatch_preserves_live_evidence(self):
        candles = []
        for index in range(9):
            close = 100.0 + (index % 2) * 0.5
            candles.append({"open_time_ms": index, "high": close + 0.2,
                            "low": close - 0.2, "close": close, "volume": 100.0})
        candles[-1] = {"open_time_ms": 8, "high": 101.5, "low": 100.8,
                       "close": 101.2, "volume": 200.0}
        score, maximum, verdict, evidence = score_volatility_breakout(
            {"breakout_klines": {"candles": candles}}, args(), {}
        )
        self.assertEqual(maximum, 3)
        self.assertIn(verdict, ("volatility breakout observation", "observe only"))
        self.assertTrue(any("breakout" in item for item in evidence))

    def test_funding_convergence_requires_comparable_intervals(self):
        data = {"funding_cross": {"funding": [
            {"symbol": "BTCUSDT", "exchange": "binance", "funding_rate": 0.001,
             "funding_interval_ms": 28_800_000},
            {"symbol": "BTCUSDT", "exchange": "okx", "funding_rate": -0.0005,
             "funding_interval_ms": 28_800_000},
        ]}}
        score, maximum, verdict, evidence = score_funding_convergence(data, args(), {})
        self.assertEqual((score, maximum), (1, 1))
        self.assertEqual(verdict, "funding differential observation")
        self.assertTrue(any("gross spread" in item for item in evidence))


if __name__ == "__main__":
    unittest.main()
