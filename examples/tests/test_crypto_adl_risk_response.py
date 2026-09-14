"""Deterministic tests for Binance ADL-risk response replay."""

import unittest

from crypto_adl_risk_monitor import classify_risk, summarize
from crypto_adl_risk_response_replay import summarize_records


def record(ts, price, risk):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price}, "adl_rows": [{"symbol": "BTCUSDT", "adl_risk": risk}],
    }}


class AdlRiskResponseTests(unittest.TestCase):
    def test_monitor_preserves_provider_risk_states(self):
        self.assertEqual(classify_risk("HIGH"), "adl_risk_high")
        self.assertEqual(classify_risk("medium"), "adl_risk_medium")
        self.assertEqual(classify_risk("low"), "adl_risk_low")
        self.assertEqual(summarize([{"symbol": "BTCUSDT", "adl_risk": "high"}])["rows"], 1)

    def test_replay_compares_high_and_low_context(self):
        rows = [record(1, 100, "high"), record(2, 101, "low"),
                record(3, 102, "low"), record(4, 110, "low")]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["by_state"]["adl_risk_high_context"]["observations"], 1)
        self.assertEqual(result["verdict"], "adl_risk_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["adl_risk_high_context"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_remains_observe_only_without_price_alignment(self):
        rows = [record(1, None, "high"), record(2, 101, "low"),
                record(3, 102, "low"), record(4, 103, "low")]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_adl_risk_observations")


if __name__ == "__main__":
    unittest.main()
