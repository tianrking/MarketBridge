"""Deterministic tests for liquidity-sandwich response research."""

import unittest

from crypto_liquidity_sandwich_monitor import book_metrics, classify_sandwich
from crypto_liquidity_sandwich_response_replay import summarize_records


def record(ts, price, state):
    return {"recorded_at_ms": ts, "observation": {
        "state": state,
        "response_price": {"price": price} if price is not None else None,
    }}


class LiquiditySandwichResponseTests(unittest.TestCase):
    def test_monitor_classifies_symmetric_near_touch_depth(self):
        book = {
            "bids": [{"price": 99.99, "qty": 2_000.0}, {"price": 99.95, "qty": 1_000.0}],
            "asks": [{"price": 100.01, "qty": 2_000.0}, {"price": 100.05, "qty": 1_000.0}],
        }
        metrics = book_metrics(book, 10.0)
        self.assertEqual(classify_sandwich(metrics, 100_000.0, 0.5, 2.0),
                         "liquidity_sandwich")
        self.assertGreater(metrics["depth_symmetry_ratio"], 0.99)

    def test_monitor_keeps_one_sided_book_ordinary(self):
        book = {
            "bids": [{"price": 99.99, "qty": 10.0}],
            "asks": [{"price": 100.01, "qty": 2_000.0}],
        }
        metrics = book_metrics(book, 10.0)
        self.assertEqual(classify_sandwich(metrics, 100_000.0, 0.5, 2.0),
                         "ordinary_book_context")

    def test_replay_compares_sandwich_and_ordinary_response(self):
        rows = [
            record(1, 100.0, "liquidity_sandwich"),
            record(2, 100.5, "ordinary_book_context"),
            record(3, 101.0, "ordinary_book_context"),
            record(4, 110.0, "ordinary_book_context"),
        ]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["by_state"]["liquidity_sandwich"]["observations"], 1)
        self.assertEqual(result["verdict"], "liquidity_sandwich_response_reported")
        self.assertAlmostEqual(
            result["by_state"]["liquidity_sandwich"]["mean_absolute_forward_return_pct"],
            10.0,
        )

    def test_replay_stays_observe_only_without_quotes(self):
        rows = [record(1, None, "liquidity_sandwich"),
                record(2, 101.0, "ordinary_book_context"),
                record(3, 102.0, "ordinary_book_context"),
                record(4, 103.0, "ordinary_book_context")]
        result = summarize_records(rows, 3, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"],
                         "observe_only_insufficient_liquidity_sandwich_observations")


if __name__ == "__main__":
    unittest.main()
