"""Deterministic tests for the short-squeeze reversal research monitor."""

import unittest

from crypto_short_squeeze_reversal_monitor import classify, price_structure, signal_payload


def candle(ts_ms, high, low, close):
    return {"ts_ms": ts_ms, "open": low, "high": high, "low": low, "close": close}


class ShortSqueezeReversalTests(unittest.TestCase):
    def test_active_squeeze_is_never_a_short_candidate(self):
        state = {"long_squeeze": {"state": "triggered_long_squeeze", "score": 9},
                 "metrics": {"buy_liquidation_notional_15m": 1000,
                             "open_interest_changes": [{"window_ms": 900000, "change_pct": -5}],
                             "funding_rate": 0.0,
                             "price_changes": [{"window_ms": 900000, "change_pct": -1}]}}
        result = classify(state, {"confirmed": True}, min_liquidation_notional=1)
        self.assertEqual(result["verdict"], "squeeze_active_no_short")

    def test_reversal_requires_deleveraging_funding_and_structure(self):
        state = {"long_squeeze": {"state": "fuel_exhaustion", "score": 7},
                 "metrics": {"buy_liquidation_notional_15m": 1000,
                             "open_interest_changes": [{"window_ms": 900000, "change_pct": -5}],
                             "funding_rate": 0.0001,
                             "price_changes": [{"window_ms": 900000, "change_pct": -1}]}}
        result = classify(state, {"confirmed": True}, min_liquidation_notional=1)
        self.assertEqual(result["verdict"], "reversal_confirmed_research_candidate")

    def test_price_structure_needs_failed_retest_and_lower_low(self):
        rows = [candle(index, 100 + index, 99 + index, 99.5 + index) for index in range(5)]
        rows[-2] = candle(3, 110, 106, 108)
        rows[-1] = candle(4, 108, 104, 105)
        result = price_structure(rows, lookback_bars=3)
        self.assertTrue(result["confirmed"])
        self.assertEqual(result["state"], "lower_high_lower_low")

    def test_missing_evidence_stays_observe_only(self):
        result = classify({"long_squeeze": {"score": 8}, "metrics": {}},
                          {"confirmed": False})
        self.assertEqual(result["verdict"], "observe_only")

    def test_signal_payload_is_only_created_for_confirmed_candidate(self):
        decision = {"verdict": "reversal_confirmed_research_candidate",
                    "evidence": ["open_interest_deleveraging"], "inputs": {"oi_change_15m_pct": -5}}
        signal = signal_payload(decision, {"confirmed": True}, "FILUSDT", "binance", 123)
        self.assertEqual(signal["execution"], "research_only_no_orders")
        self.assertIsNone(signal_payload({"verdict": "squeeze_active_no_short"}, {}, "FILUSDT", "binance", 123))

    def test_optional_liquidation_volume_gate_requires_core_ratio(self):
        state = {"long_squeeze": {"state": "fuel_exhaustion", "score": 7},
                 "metrics": {"buy_liquidation_notional_15m": 1000,
                             "liquidation_to_perp_volume_ratio_15m": 0.01,
                             "open_interest_changes": [{"window_ms": 900000, "change_pct": -5}],
                             "funding_rate": 0.0,
                             "price_changes": [{"window_ms": 900000, "change_pct": -1}]}}
        result = classify(state, {"confirmed": True}, min_liquidation_notional=1,
                          min_liquidation_volume_ratio=0.02)
        self.assertEqual(result["verdict"], "observe_only")


if __name__ == "__main__":
    unittest.main()
