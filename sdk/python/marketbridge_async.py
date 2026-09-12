"""Async research API, bounded concurrency, explicit durable cursor recovery.

No implicit mutation retries, redirects, trading methods or hidden credentials.
Use one async context manager per application to reuse the connection pool.
"""
from __future__ import annotations
import asyncio
import json
import math
from typing import Any, AsyncIterator
import httpx
from marketbridge import MarketBridge


class AsyncMarketBridge:
    def __init__(self, base_url: str = "http://127.0.0.1:8080", *,
                 api_key: str | None = None, timeout: float = 15,
                 max_concurrency: int = 2, transport=None):
        checked = MarketBridge(base_url, api_key=api_key, timeout=timeout)
        if type(max_concurrency) is not int or not 1 <= max_concurrency <= 32:
            raise ValueError("max_concurrency must be 1..32")
        self._gate = asyncio.Semaphore(max_concurrency)
        self._timeout = timeout
        self._base = checked.base_url
        self._client = httpx.AsyncClient(
            headers={"x-api-key": api_key} if api_key else {},
            timeout=timeout, follow_redirects=False, trust_env=False,
            limits=httpx.Limits(max_connections=max_concurrency), transport=transport)

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        await self.aclose()

    async def aclose(self):
        await self._client.aclose()

    async def _request(self, method: str, path: str, payload=None, params=None) -> dict[str, Any]:
        if not path.startswith("/v1/") or "?" in path or "#" in path or ".." in path:
            raise ValueError("only versioned API paths are accepted")
        data = None if payload is None else json.dumps(payload, allow_nan=False).encode()
        if data is not None and len(data) > 2 * 1024 * 1024:
            raise ValueError("request exceeds 2 MiB")

        async def send():
            async with self._gate:
                async with self._client.stream(method, self._base + path,
                        content=data, params=params, headers={"Content-Type": "application/json"}) as response:
                    response.raise_for_status()
                    body = bytearray()
                    async for chunk in response.aiter_bytes():
                        body.extend(chunk)
                        if len(body) > 8 * 1024 * 1024:
                            raise ValueError("response exceeds 8 MiB")
                    result = json.loads(body, parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))
                    if not isinstance(result, dict):
                        raise ValueError("API response must be an object")
                    return result
        # Includes pool/semaphore wait, not only per-chunk inactivity timeout.
        return await asyncio.wait_for(send(), self._timeout)

    async def get(self, path: str, *, params: dict[str, Any] | None = None) -> dict[str, Any]:
        return await self._request("GET", path, params=params)

    async def evaluate(self, evidence):
        return await self._request("POST", "/v1/research/evaluate", evidence)

    async def evaluate_live(self, route):
        return await self._request("POST", "/v1/research/evaluate-live", route)

    async def replay(self, frames):
        return await self._request("POST", "/v1/research/replay", {"frames": frames})

    async def paper(self, initial, frames):
        return await self._request("POST", "/v1/research/paper", {"initial": initial, "frames": frames})

    async def scan(self, candidates, *, as_of_ms, min_net_bps=0):
        return await self._request("POST", "/v1/research/scan", {"candidates": candidates, "as_of_ms": as_of_ms, "min_net_bps": min_net_bps})

    async def scan_live(self, candidates, *, min_net_bps=0):
        return await self._request("POST", "/v1/research/scan-live", {"candidates": candidates, "min_net_bps": min_net_bps})

    async def workspace(self, action, request=None):
        payload = {"action": action}
        if request is not None:
            payload["request"] = request
        return await self._request("POST", "/v1/research/workspace", payload)

    async def run(self, run_id, model, inputs):
        return await self.workspace("run", {"id": run_id, "model": model, "input": inputs})

    async def configure(self, config):
        return await self._request("POST", "/v1/research/control", config)

    async def documents(self, namespace: str, *, after_sequence: int = 0,
                        poll_seconds: float = 2, follow: bool = False) -> AsyncIterator[dict[str, Any]]:
        """Yield durable rows in order. Save sequence AFTER processing a row.

        Exceptions propagate; reconnect with the last processed sequence. At-least-once
        recovery requires caller idempotence. No silent retry storms on 401/429.
        """
        if type(after_sequence) is not int or after_sequence < 0:
            raise ValueError("invalid cursor")
        if not math.isfinite(poll_seconds) or poll_seconds < 0.25:
            raise ValueError("poll_seconds must be >= 0.25")
        cursor = after_sequence
        while True:
            page = await self.workspace("list", {"namespace": namespace, "after_sequence": cursor, "limit": 100})
            rows = page["documents"]
            for row in rows:
                sequence = row["sequence"]
                if type(sequence) is not int or sequence <= cursor:
                    raise ValueError("non-monotonic server cursor")
                yield row
                cursor = sequence
            if page["next_sequence"] != cursor:
                raise ValueError("server cursor disagrees with rows")
            if not rows:
                if not follow:
                    return
                await asyncio.sleep(poll_seconds)
