"""Deterministic tests for prior-day pivot response replay."""

import unittest

from crypto_pivot_response_replay import (
    build_observations,
    pivot_features,
    pivot_series,
    summarize,
    traditional_levels,
)


DAY = 86_400_000


def bar(day, hour, close, high=None, low=None):
    return {"ts_ms": day * DAY + hour * 3_600_000, "open": close,
            "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close}


class PivotResponseTests(unittest.TestCase):
    def test_traditional_levels_use_prior_ohlc(self):
        levels = traditional_levels({"high": 110.0, "low": 90.0, "close": 100.0})
        self.assertEqual(levels["pivot"], 100.0)
        self.assertEqual(levels["r1"], 110.0)
        self.assertEqual(levels["s1"], 90.0)

    def test_first_day_has_no_pivot_and_next_day_uses_prior_day(self):
        rows = [bar(0, hour, 100.0 + hour, 101.0 + hour, 99.0 + hour)
                for hour in range(3)]
        rows.extend([bar(1, hour, 105.0, 106.0, 104.0) for hour in range(3)])
        series = pivot_series(rows, 10.0)
        self.assertIsNone(series[0])
        self.assertIsNotNone(series[3])
        self.assertAlmostEqual(series[3]["levels"]["pivot"], (103.0 + 99.0 + 102.0) / 3.0)
        self.assertIsNone(pivot_features(rows, 0))

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(0, hour, 100.0 + hour) for hour in range(4)]
        rows.extend([bar(1, hour, 105.0) for hour in range(4)])
        before = pivot_features(rows, 4)
        rows[-1] = bar(1, 3, 1000.0, 1001.0, 999.0)
        after = pivot_features(rows, 4)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_event_and_control(self):
        rows = [bar(day, hour, 100.0 + (hour % 4))
                for day in range(4) for hour in range(6)]
        observations = build_observations(rows, 10.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_max_path_return_pct", observations[0])
        manual = [
            {"event": "s1_reclaim", "state": "inside_pivot_range", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"event": None, "state": "inside_pivot_range", "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "pivot_response_reported")
        self.assertEqual(result["pivot_event_observations"], 1)


if __name__ == "__main__":
    unittest.main()
