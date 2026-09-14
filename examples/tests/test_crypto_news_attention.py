#!/usr/bin/env python3
"""Deterministic tests for CryptoPanic attention monitoring and replay."""

import unittest

from crypto_news_attention_monitor import summarize_news
from crypto_news_attention_replay import summarize_records


def news(score, title):
    return [{
        "source": "cryptopanic", "category": "news", "metric": "news_item",
        "value": score, "title": title, "url": f"https://news.example/{title}",
    }]


def record(state, price, timestamp):
    return {
        "recorded_at_ms": timestamp,
        "observation": {"news": {"state": state}, "price": {"price": price}},
    }


class NewsAttentionTests(unittest.TestCase):
    def test_duplicate_urls_are_not_double_counted(self):
        rows = news(4, "same") + news(-4, "same") + news(1, "small")
        result = summarize_news(rows, min_score=3, min_items=1)
        self.assertEqual(result["item_count"], 2)
        self.assertEqual(result["strong_item_count"], 1)
        self.assertEqual(result["state"], "news_attention_shock")

    def test_missing_feed_is_observe_only(self):
        result = summarize_news([], min_score=3, min_items=2)
        self.assertEqual(result["state"], "observe_only_missing_news")

    def test_shock_forward_movement_is_aligned(self):
        result = summarize_records([
            record("news_attention_shock", 100, 1),
            record("normal_news_activity", 105, 2),
            record("normal_news_activity", 120, 3),
            record("normal_news_activity", 121, 4),
        ], horizon_records=1, min_observations=1)
        self.assertEqual(result["aligned_forward_windows"], 3)
        self.assertEqual(result["attention_shock"]["observations"], 1)
        self.assertEqual(result["verdict"], "news_attention_forward_response_reported")

    def test_insufficient_shocks_do_not_promote(self):
        result = summarize_records([
            record("news_attention_shock", 100, 1),
            record("normal_news_activity", 101, 2),
        ], horizon_records=1, min_observations=2)
        self.assertEqual(result["verdict"], "observe_only_insufficient_news_shocks")


if __name__ == "__main__":
    unittest.main()
