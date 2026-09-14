"""Deterministic tests for historical stablecoin supply replay."""

import unittest

from crypto_stablecoin_historical_response_replay import (
    aligned_observations,
    candle_points,
    stablecoin_points,
    summarize,
)


class StablecoinHistoricalReplayTests(unittest.TestCase):
    def test_points_normalize_provider_rows_to_utc_dates(self):
        stable = stablecoin_points({"rows": [
            {"ts_ms": 1_704_067_200_000, "total_circulating_usd": 100.0},
            {"ts_ms": 1_704_153_600_000, "total_circulating_usd": 101.0},
        ]})
        prices = candle_points({"candles": [
            {"open_time_ms": 1_704_067_200_000, "close": 100.0},
            {"open_time_ms": 1_704_153_600_000, "close": 101.0},
        ]})
        self.assertEqual(stable[0][0], "2024-01-01")
        self.assertEqual(prices[1][1][0], 101.0)

    def test_expansion_and_contraction_states_use_trailing_rows(self):
        stable = [("2024-01-01", (100.0, 1)), ("2024-01-02", (102.0, 2)),
                  ("2024-01-03", (98.0, 3)), ("2024-01-04", (98.0, 4))]
        prices = [("2024-01-01", (100.0, 1)), ("2024-01-02", (101.0, 2)),
                  ("2024-01-03", (99.0, 3)), ("2024-01-04", (100.0, 4)),
                  ("2024-01-05", (102.0, 5))]
        rows = aligned_observations(stable, prices, 1, 1.0, 1)
        self.assertEqual([row["state"] for row in rows],
                         ["stablecoin_supply_expansion", "stablecoin_supply_contraction",
                          "stablecoin_supply_flat"])
        self.assertAlmostEqual(rows[0]["change_pct"], 2.0)

    def test_incomplete_window_and_future_price_are_skipped(self):
        stable = [("2024-01-01", (100.0, 1)), ("2024-01-02", (101.0, 2))]
        prices = [("2024-01-01", (100.0, 1)), ("2024-01-02", (101.0, 2))]
        self.assertEqual(aligned_observations(stable, prices, 2, 1.0, 1), [])

    def test_summary_stays_observe_only_with_short_qualifying_sample(self):
        result = summarize([{
            "state": "stablecoin_supply_expansion",
            "change_pct": 2.0,
            "forward_return_pct": 1.0,
            "forward_abs_return_pct": 1.0,
        }], 2)
        self.assertEqual(result["verdict"],
                         "observe_only_insufficient_historical_supply_windows")


if __name__ == "__main__":
    unittest.main()
