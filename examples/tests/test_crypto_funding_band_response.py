"""Deterministic tests for provider funding-band response replay."""

import unittest

from crypto_funding_band_monitor import classify_band
from crypto_funding_band_response_replay import summarize_records


def funding_row(state):
    return {"symbol": "BTCUSDT", "state": state}


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price} if price is not None else None,
        "funding_rows": [funding_row(state)],
    }}


class FundingBandResponseTests(unittest.TestCase):
    def test_monitor_classifies_provider_band_states(self):
        row = {"funding_rate": 0.0008, "funding_rate_cap": 0.001,
               "funding_rate_floor": -0.001}
        self.assertEqual(classify_band(row, 0.8)["state"], "near_upper_funding_cap")
        row["funding_rate"] = -0.0008
        self.assertEqual(classify_band(row, 0.8)["state"], "near_lower_funding_floor")

    def test_replay_compares_upper_cap_and_ordinary_states(self):
        rows = [
            record(1, 100, "near_upper_funding_cap"),
            record(2, 101, "within_provider_funding_band"),
            record(3, 102, "within_provider_funding_band"),
            record(4, 110, "within_provider_funding_band"),
        ]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["by_state"]["near_upper_funding_cap"]["observations"], 1)
        self.assertEqual(result["verdict"], "funding_band_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["near_upper_funding_cap"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_stays_observe_only_without_price_alignment(self):
        rows = [record(1, None, "near_upper_funding_cap"),
                record(2, 101, "within_provider_funding_band"),
                record(3, 102, "within_provider_funding_band"),
                record(4, 103, "within_provider_funding_band")]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"],
                         "observe_only_insufficient_funding_band_observations")


if __name__ == "__main__":
    unittest.main()
