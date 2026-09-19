"""Deterministic tests for the 24-hour roll-out replay."""

import unittest

from crypto_24h_rollout_response_replay import HOUR_MS, candle_rows, rollout_events, summarize


def candle(ts, close):
    return {"open_time_ms": ts, "open": close, "close": close,
            "high": close, "low": close}


class RolloutReplayTests(unittest.TestCase):
    def test_old_positive_extreme_maps_to_short(self):
        rows = [candle(index * HOUR_MS, 100.0) for index in range(60)]
        rows[24]["close"] = 110.0
        for index in range(25, 49):
            rows[index]["open"] = rows[index - 1]["close"]
            rows[index]["close"] = rows[index - 1]["close"]
        # The 24-hour-old candle at entry 48 is the largest positive return.
        rows[48]["close"] = 99.0
        rows[49]["open"] = 99.0
        rows[49]["close"] = 98.0
        events = rollout_events(candle_rows({"candles": rows}))
        event = next(item for item in events if item["ts_ms"] == 48 * HOUR_MS)
        self.assertEqual(event["side"], "short")
        self.assertAlmostEqual(event["direction_return_pct"], (1 - 98 / 99) * 100)

    def test_summary_is_observe_only_below_minimum(self):
        result = summarize([], 2, 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_rollout_observations")

    def test_summary_reports_sides(self):
        events = [
            {"side": "short", "direction_return_pct": 1.0},
            {"side": "long", "direction_return_pct": -0.5},
        ]
        result = summarize(events, 1, 0)
        self.assertEqual(result["verdict"], "rollout_response_reported")
        self.assertEqual(result["short"]["observations"], 1)
        self.assertEqual(result["long"]["observations"], 1)


if __name__ == "__main__":
    unittest.main()
