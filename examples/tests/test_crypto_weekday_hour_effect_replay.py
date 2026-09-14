"""Deterministic tests for the weekday/hour effect replay."""

import unittest

from crypto_weekday_hour_effect_replay import candle_rows, event_rows, summarize


HOUR = 3_600_000


def candle(ts, open_price, close):
    return {"open_time_ms": ts, "open": open_price, "close": close,
            "high": max(open_price, close), "low": min(open_price, close)}


class WeekdayHourEffectReplayTests(unittest.TestCase):
    def test_target_and_same_hour_control_are_separated(self):
        # 2026-09-08 is Tuesday; 2026-09-09 is Wednesday.
        tuesday = 1788829200000 + 4 * HOUR  # 2026-09-08 05:00 UTC
        wednesday = tuesday + 24 * HOUR
        payload = {"candles": [
            candle(tuesday, 100, 99), candle(tuesday + HOUR, 99, 100),
            candle(tuesday + 8 * HOUR, 99, 98),
            candle(wednesday, 100, 100.5), candle(wednesday + HOUR, 100.5, 100.5),
            candle(wednesday + 8 * HOUR, 100.5, 101),
        ]}
        rows = candle_rows(payload)
        events = event_rows(rows, 1, 5, 8, 1)
        self.assertEqual([event["bucket"] for event in events], ["target", "same_hour_control"])
        self.assertTrue(events[0]["red_event"])
        self.assertAlmostEqual(events[0]["bounce_return_pct"], (100 / 99 - 1) * 100)

    def test_summary_requires_both_target_and_control_samples(self):
        result = summarize([{"bucket": "target", "event_return_pct": -1,
                             "bounce_return_pct": 0.1, "forward_return_pct": -2,
                             "red_event": True}], 2, 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_matched_clock_observations")

    def test_summary_reports_target_minus_control_difference(self):
        rows = []
        for bucket, forward in (("target", -2.0), ("same_hour_control", 1.0)):
            rows.append({"bucket": bucket, "event_return_pct": -1.0,
                         "bounce_return_pct": 0.1, "forward_return_pct": forward,
                         "red_event": True})
        result = summarize(rows, 1, 0)
        self.assertAlmostEqual(result["forward_mean_difference_target_minus_control_pct"], -3.0)
        self.assertEqual(result["verdict"], "weekday_hour_effect_reported")


if __name__ == "__main__":
    unittest.main()
