"""Deterministic tests for stablecoin rotation response replay."""

import unittest

from crypto_stablecoin_rotation_response_replay import summarize_records


def record(ts, price, symbol, mid):
    return {"recorded_at_ms": ts, "observation": {
        "risk_asset": {"mid": price},
        "stablecoin_quotes": [{"base": symbol[:4], "counter": symbol[4:], "mid": mid}],
    }}


class StablecoinRotationResponseReplayTests(unittest.TestCase):
    def test_usdc_discount_aligns_with_positive_btc_return(self):
        result = summarize_records([
            record(1, 100, "USDCUSDT", 0.999), record(2, 101, "USDCUSDT", 1.0),
            record(3, 102, "USDCUSDT", 1.0), record(4, 110, "USDCUSDT", 1.0),
        ], 3, 5, 1)
        bucket = result["by_state"]["usdc_discount"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_aligned_return_bps"], 1000.0)
        self.assertEqual(result["verdict"], "stablecoin_rotation_response_reported")

    def test_inverse_usdt_usdc_quote_is_normalized(self):
        result = summarize_records([
            record(1, 100, "USDTUSDC", 1.001), record(2, 101, "USDTUSDC", 1.0),
            record(3, 102, "USDTUSDC", 1.0), record(4, 110, "USDTUSDC", 1.0),
        ], 3, 5, 1)
        self.assertEqual(result["by_state"]["usdc_discount"]["observations"], 1)
        self.assertGreater(result["by_state"]["usdc_discount"]["mean_aligned_return_bps"], 0)

    def test_missing_quote_remains_observe_only(self):
        rows = [record(1, 100, "DAIUSDT", 1.0), record(2, 101, "DAIUSDT", 1.0),
                record(3, 102, "DAIUSDT", 1.0), record(4, 103, "DAIUSDT", 1.0)]
        result = summarize_records(rows, 3, 5, 1)
        self.assertEqual(result["directional_usdc_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_directional_usdc_windows")


if __name__ == "__main__":
    unittest.main()
