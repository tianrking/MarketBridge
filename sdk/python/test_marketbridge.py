import io
import json
import unittest
from unittest.mock import patch

from marketbridge import MarketBridge, _NoRedirect


class ClientTests(unittest.TestCase):
    def test_rejects_unsafe_or_invalid_base_urls(self):
        for url in ("file:///tmp/x", "https://user:pass@example.test", "https://example.test?q=secret"):
            with self.assertRaises(ValueError):
                MarketBridge(url)

    def test_rejects_nonfinite_values_before_network(self):
        with self.assertRaises(ValueError):
            MarketBridge().evaluate({"quantity": float("nan")})
        with self.assertRaises(ValueError):
            MarketBridge(timeout=float("inf"))

    def test_versioned_request_and_key_header(self):
        with patch("marketbridge.build_opener") as opener:
            opener.return_value.open.return_value = io.BytesIO(b'{"model_version":"test"}')
            result = MarketBridge(api_key="fixture-key").replay([{"as_of_ms": 10}])
            request = opener.return_value.open.call_args.args[0]
            self.assertEqual(request.full_url, "http://127.0.0.1:8080/v1/research/replay")
            self.assertEqual(request.get_header("X-api-key"), "fixture-key")
            self.assertEqual(json.loads(request.data), {"frames": [{"as_of_ms": 10}]})
            self.assertEqual(result["model_version"], "test")

    def test_redirects_never_forward_api_keys(self):
        self.assertIsNone(_NoRedirect().redirect_request(None, None, 302, "", {}, "https://other.test"))

    def test_paper_payload(self):
        with patch("marketbridge.build_opener") as opener:
            opener.return_value.open.return_value = io.BytesIO(b'{}')
            MarketBridge().paper({"buy_venue_quote": 100}, [{"size_index": 0}])
            request = opener.return_value.open.call_args.args[0]
            self.assertTrue(request.full_url.endswith("/v1/research/paper"))
            self.assertEqual(json.loads(request.data), {"initial": {"buy_venue_quote": 100}, "frames": [{"size_index": 0}]})


if __name__ == "__main__":
    unittest.main()
