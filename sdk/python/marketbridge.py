"""Dependency-free synchronous research client. No trading methods.

Run with Python 3.10+. These endpoints accept evidence, not order instructions.
The full WS/cursor client and generated typed instrument models are roadmap work.
"""
from __future__ import annotations

import json
import math
from typing import Any
from urllib.parse import urlsplit
from urllib.request import Request, HTTPRedirectHandler, build_opener


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Never forward API keys to a redirect destination.
        return None


class MarketBridge:
    def __init__(self, base_url: str = "http://127.0.0.1:8080", *, api_key: str | None = None,
                 timeout: float = 15.0):
        parsed = urlsplit(base_url)
        if parsed.scheme not in {"http", "https"} or not parsed.netloc:
            raise ValueError("base_url must be an absolute HTTP(S) URL")
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ValueError("credentials/query/fragment are not allowed in base_url")
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("timeout must be positive")
        self.base_url, self.api_key, self.timeout = base_url.rstrip("/"), api_key, timeout

    def _post(self, path: str, payload: dict[str, Any]) -> dict[str, Any]:
        headers = {"Content-Type": "application/json", "Accept": "application/json"}
        if self.api_key:
            headers["x-api-key"] = self.api_key
        request = Request(self.base_url + path, method="POST", headers=headers,
                          data=json.dumps(payload, allow_nan=False).encode("utf-8"))
        # Do not retry implicitly: keep rate limits and experiment counts explicit.
        with build_opener(_NoRedirect()).open(request, timeout=self.timeout) as response:
            return json.load(response)

    def evaluate(self, evidence: dict[str, Any]) -> dict[str, Any]:
        return self._post("/v1/research/evaluate", evidence)

    def replay(self, frames: list[dict[str, Any]]) -> dict[str, Any]:
        return self._post("/v1/research/replay", {"frames": frames})

    def paper(self, initial: dict[str, float], frames: list[dict[str, Any]]) -> dict[str, Any]:
        return self._post("/v1/research/paper", {"initial": initial, "frames": frames})
