"""Deterministic tests for the VWAP deviation reversion replay."""

import unittest

from crypto_vwap_deviation_reversion_replay import reversion_events, summarize


def candle(timestamp, close, volume=1.0):
    return {"ts_ms": timestamp, "close": close, "high": close, "low": close, "volume": volume}


class VWAPDeviationReplayTests(unittest.TestCase):
    def test_summary_applies_direction_and_cost(self):
        events = [
            {"direction_sign": 1, "aligned_return_bps": 100.0},
            {"direction_sign": -1, "aligned_return_bps": 50.0},
        ]
        result = summarize(events, 10.0, 0.0, 2)
        self.assertEqual(result["signals"], 2)
        self.assertAlmostEqual(result["mean_cost_adjusted_return_bps"], 65.0)
        self.assertEqual(result["verdict"], "vwap_deviation_response_reported")

    def test_missing_events_remain_observe_only(self):
        result = summarize([], 0.0, 0.0, 1)
        self.assertEqual(result["signals"], 0)
        self.assertEqual(result["verdict"], "observe_only")

    def test_cross_back_can_create_long_signal(self):
        rows = [candle(0, 100), candle(3_600_000, 90), candle(7_200_000, 101),
                candle(10_800_000, 102)]
        events = reversion_events(rows, 50.0, 0.0, 1)
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["direction"], "long")


if __name__ == "__main__":
    unittest.main()
