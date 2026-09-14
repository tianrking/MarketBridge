#!/usr/bin/env python3
"""Deterministic tests for cross-venue funding-spread response replay."""

import unittest

from crypto_funding_spread_response_replay import (
    annualized_bps,
    asof_funding,
    spread_observations,
    summarize,
)


class FundingSpreadResponseTests(unittest.TestCase):
    def test_annualizes_using_each_point_in_time_interval(self):
        self.assertAlmostEqual(annualized_bps(0.001, 8 * 60 * 60 * 1000), 10950.0)
        self.assertIsNone(annualized_bps(0.001, 0))

    def test_stale_funding_is_not_carried_forward(self):
        points = [(1_000, 0.001, 100)]
        self.assertIsNone(asof_funding(points, 1_101, 1.0))
        self.assertEqual(asof_funding(points, 1_100, 1.0), points[0])

    def test_extreme_spread_is_compared_with_ordinary_control(self):
        a = [(1_000, 0.001, 100), (2_000, 0.001, 100)]
        b = [(1_000, 0.0, 100), (2_000, 0.0, 100)]
        prices = [(1_000, 100.0), (2_000, 101.0), (3_000, 105.0), (4_000, 105.0)]
        rows = spread_observations(a, b, prices, 1, 50_000.0, 0.0, 2.0)
        self.assertEqual([row["state"] for row in rows], ["extreme_spread", "extreme_spread"])
        # An ordinary control can be made from a lower threshold replay.
        control = spread_observations(a, b, prices, 1, 4_000_000_000.0, 0.0, 2.0)
        self.assertEqual([row["state"] for row in control], ["ordinary_spread", "ordinary_spread"])

    def test_summary_requires_both_event_and_control_evidence(self):
        event = {"state": "extreme_spread", "absolute_forward_return_pct": 0.05,
                 "forward_return_pct": 0.05}
        ordinary = {"state": "ordinary_spread", "absolute_forward_return_pct": 0.01,
                    "forward_return_pct": -0.01}
        result = summarize([event, ordinary], min_observations=1,
                           paper_cost_bps=1.0, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "funding_spread_stress_response_candidate")
        self.assertAlmostEqual(result["absolute_response_edge_bps"], 4.0)

    def test_empty_summary_remains_observe_only(self):
        result = summarize([], min_observations=1, paper_cost_bps=0.0, min_edge_bps=0.0)
        self.assertEqual(result["verdict"], "observe_only")
        self.assertIsNone(result["absolute_response_edge_bps"])


if __name__ == "__main__":
    unittest.main()
