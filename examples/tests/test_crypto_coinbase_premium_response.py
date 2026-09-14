"""Deterministic tests for Coinbase premium response research."""

import unittest

from crypto_coinbase_premium_monitor import classify_premium, find_quote, quote_price
from crypto_coinbase_premium_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "state": state,
        "reference_quote": {"price": price} if price is not None else None,
    }}


class CoinbasePremiumResponseTests(unittest.TestCase):
    def test_monitor_classifies_premium_discount_and_ordinary(self):
        self.assertEqual(classify_premium(6.0, 5.0), "coinbase_premium")
        self.assertEqual(classify_premium(-6.0, 5.0), "coinbase_discount")
        self.assertEqual(classify_premium(1.0, 5.0), "ordinary_coinbase_reference_spread")

    def test_monitor_matches_normalized_quote_rows(self):
        payload = {"quotes": [{
            "instrument_ref": {"symbol": "BTC-USD", "product_type": "spot"},
            "source_ref": {"source": "coinbase"},
            "payload": {"bid": 99.0, "ask": 101.0},
        }]}
        row = find_quote(payload, "BTC-USD", "coinbase")
        self.assertEqual(row["price"], 100.0)
        self.assertEqual(quote_price(payload["quotes"][0])["price"], 100.0)

    def test_replay_compares_premium_and_ordinary_response(self):
        rows = [
            record(1, 100.0, "coinbase_premium"),
            record(2, 100.5, "ordinary_coinbase_reference_spread"),
            record(3, 101.0, "ordinary_coinbase_reference_spread"),
            record(4, 110.0, "ordinary_coinbase_reference_spread"),
        ]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["by_state"]["coinbase_premium"]["observations"], 1)
        self.assertEqual(result["verdict"], "coinbase_premium_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["coinbase_premium"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_stays_observe_only_without_quotes(self):
        rows = [record(1, None, "coinbase_premium"),
                record(2, 101.0, "ordinary_coinbase_reference_spread"),
                record(3, 102.0, "ordinary_coinbase_reference_spread"),
                record(4, 103.0, "ordinary_coinbase_reference_spread")]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"],
                         "observe_only_insufficient_coinbase_premium_observations")


if __name__ == "__main__":
    unittest.main()
