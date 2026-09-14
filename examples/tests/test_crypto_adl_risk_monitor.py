import unittest

from crypto_adl_risk_monitor import classify_risk, summarize


class AdlRiskMonitorTests(unittest.TestCase):
    def test_risk_levels_are_non_directional_context(self):
        self.assertEqual(classify_risk("HIGH"), "adl_risk_high")
        self.assertEqual(classify_risk("medium"), "adl_risk_medium")
        self.assertEqual(classify_risk("unknown"), "observe_only_unknown_adl_risk")

    def test_summary_keeps_high_symbols_visible(self):
        result = summarize([
            {"symbol": "BTCUSDT", "adl_risk": "high"},
            {"symbol": "ETHUSDT", "adl_risk": "low"},
        ])
        self.assertEqual(result["by_state"]["adl_risk_high"], 1)
        self.assertEqual(result["high_risk_symbols"], ["BTCUSDT"])


if __name__ == "__main__":
    unittest.main()
