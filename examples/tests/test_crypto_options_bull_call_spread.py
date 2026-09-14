#!/usr/bin/env python3
"""Deterministic tests for the bull-call-spread research case."""

import unittest

from crypto_options_bull_call_spread_monitor import summarize_spread
from crypto_options_bull_call_spread_replay import summarize_records


def option(strike, ask, bid, mark, expiry="2099-01-01T00:00:00Z"):
    return {"option_type": "call", "strike": strike, "expiry_time": expiry,
            "underlying_price": 100.0, "ask_price": ask, "bid_price": bid,
            "mark_price": mark, "mark_iv": 60.0, "open_interest": 10.0}


def record(state, timestamp, debit=4.0):
    return {"recorded_at_ms": timestamp, "observation": {"target_expiry": {
        "expiry_time": "2099-01-01T00:00:00Z", "state": state, "debit": debit,
        "width": 10.0, "legs": {"long_call": {"strike": 95.0}, "short_call": {"strike": 105.0}},
    }}}


class BullCallSpreadTests(unittest.TestCase):
    def test_quotes_produce_bounded_paper_geometry(self):
        result = summarize_spread([option(95, 6, 5, 5.5), option(105, 2.1, 2, 2.05)],
                                  "2099-01-01T00:00:00Z", 0, 0.95, 1.05)
        self.assertEqual(result["state"], "bull_call_spread_quote_available")
        self.assertAlmostEqual(result["debit"], 4.0)
        self.assertAlmostEqual(result["max_profit"], 6.0)
        self.assertAlmostEqual(result["breakeven"], 99.0)

    def test_missing_bid_uses_mark_only_and_stays_explicit(self):
        result = summarize_spread([option(95, 6, 5, 5.5), option(105, None, None, 2.0)],
                                  "2099-01-01T00:00:00Z", 0, 0.95, 1.05)
        self.assertEqual(result["state"], "bull_call_spread_mark_only")
        self.assertEqual(result["quote_quality"], "mark_only")

    def test_replay_requires_consecutive_valid_quotes(self):
        result = summarize_records([
            record("bull_call_spread_quote_available", 1),
            record("observe_only_missing_call_quotes", 2),
            record("bull_call_spread_mark_only", 3),
        ], 2)
        summary = next(iter(result["by_spread"].values()))
        self.assertEqual(summary["longest_valid_run"], 1)
        self.assertEqual(summary["verdict"], "observe_only_no_persistent_spread_quote")


if __name__ == "__main__":
    unittest.main()
