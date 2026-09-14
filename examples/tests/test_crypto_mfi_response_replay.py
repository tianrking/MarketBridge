"""Deterministic tests for MFI response replay."""

import unittest

from crypto_mfi_response_replay import (
    build_observations,
    mfi_features,
    mfi_series,
    summarize,
)


def bar(ts_ms, close, volume=10.0, high=None, low=None):
    return {"ts_ms": ts_ms, "open": close, "high": high if high is not None else close + 1.0,
            "low": low if low is not None else close - 1.0, "close": close, "volume": volume}


class MfiResponseTests(unittest.TestCase):
    def test_warmup_and_basic_money_flow_are_explicit(self):
        rows = [bar(index, 100.0 + index, volume=10.0) for index in range(5)]
        series = mfi_series(rows, 3, 80.0, 20.0)
        self.assertIsNone(series[2])
        self.assertEqual(series[3]["mfi"], 100.0)
        self.assertEqual(mfi_features(rows, 1, 3), None)

    def test_falling_typical_price_can_reach_oversold(self):
        rows = [bar(index, 140.0 - index * 2, volume=10.0,
                   high=141.0 - index * 2, low=139.0 - index * 2) for index in range(12)]
        feature = mfi_series(rows, 3, 80.0, 20.0)[-1]
        self.assertEqual(feature["state"], "oversold")
        self.assertEqual(feature["direction_sign"], 1)

    def test_oversold_reclaim_keeps_event_direction(self):
        rows = [bar(index, 100.0 - index * 2, volume=10.0) for index in range(6)]
        rows.extend([bar(6, 90.0, volume=10.0), bar(7, 110.0, volume=10.0)])
        feature = mfi_series(rows, 3, 80.0, 20.0)[-1]
        self.assertEqual(feature["event"], "oversold_reclaim")
        self.assertEqual(feature["event_direction_sign"], 1)

    def test_future_bar_mutation_does_not_change_prior_feature(self):
        rows = [bar(index, 100.0 + index * 0.5, volume=10.0) for index in range(12)]
        before = mfi_features(rows, 6, 4)
        rows[-1] = bar(11, 1000.0, volume=1000.0, high=1001.0, low=999.0)
        after = mfi_features(rows, 6, 4)
        self.assertEqual(before, after)

    def test_observations_and_summary_require_extreme_and_neutral_control(self):
        rows = [bar(index, 100.0 + index * 0.2, volume=10.0) for index in range(30)]
        observations = build_observations(rows, 4, 80.0, 20.0, 2)
        self.assertTrue(observations)
        self.assertIn("forward_min_path_return_pct", observations[0])
        manual = [
            {"state": "oversold", "event": "oversold_reclaim", "forward_return_pct": 1.0,
             "direction_aligned_return_bps": 100.0, "forward_absolute_return_pct": 1.0},
            {"state": "neutral", "event": None, "forward_return_pct": 0.1,
             "direction_aligned_return_bps": None, "forward_absolute_return_pct": 0.1},
        ]
        result = summarize(manual, 1)
        self.assertEqual(result["verdict"], "mfi_response_reported")
        self.assertEqual(result["threshold_event_observations"], 1)


if __name__ == "__main__":
    unittest.main()
