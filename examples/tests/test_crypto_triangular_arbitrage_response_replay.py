"""Deterministic tests for triangular quote response replay."""

import unittest

from crypto_triangular_arbitrage_response_replay import summarize_records


def record(ts, price, edge):
    return {"recorded_at_ms": ts, "observation": {
        "price": {"price": price},
        "best_path": ({"net_edge_bps": edge, "direction": "usdt_btc_eth_usdt"}
                       if edge is not None else None),
    }}


class TriangularResponseReplayTests(unittest.TestCase):
    def test_qualifying_triangle_edge_has_forward_response(self):
        result = summarize_records([
            record(1, 100, 10), record(2, 101, None), record(3, 102, None), record(4, 110, None),
        ], 3, 5, 1)
        bucket = result["by_state"]["qualifying_triangle_edge"]
        self.assertEqual(bucket["observations"], 1)
        self.assertAlmostEqual(bucket["mean_forward_return_pct"], 10.0)
        self.assertEqual(result["verdict"], "triangular_quote_response_reported")

    def test_below_hurdle_is_not_qualifying(self):
        result = summarize_records([
            record(1, 100, 1), record(2, 101, None), record(3, 102, None), record(4, 103, None),
        ], 3, 5, 1)
        self.assertEqual(result["by_state"]["qualifying_triangle_edge"]["observations"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_triangle_edges")

    def test_missing_price_remains_observe_only(self):
        rows = [record(1, 100, 10), record(2, 101, None), record(3, 102, None), record(4, 103, None)]
        rows[0]["observation"]["price"] = None
        result = summarize_records(rows, 3, 5, 1)
        self.assertEqual(result["aligned_forward_windows"], 0)
        self.assertEqual(result["verdict"], "observe_only_insufficient_aligned_triangle_edges")


if __name__ == "__main__":
    unittest.main()
