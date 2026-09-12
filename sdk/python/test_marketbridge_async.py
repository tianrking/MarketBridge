import asyncio
import json
import unittest
import httpx
from marketbridge_async import AsyncMarketBridge


class AsyncTests(unittest.IsolatedAsyncioTestCase):
    async def test_filtered_data_query_is_encoded(self):
        def handler(request):
            self.assertEqual(request.url.params['symbol'], 'BTC/USDT')
            return httpx.Response(200,json={"rows":[]})
        async with AsyncMarketBridge(transport=httpx.MockTransport(handler)) as client:
            self.assertEqual(await client.get('/v1/market/quotes',params={"symbol":"BTC/USDT"}),{"rows":[]})

    async def test_cursor_recovery_and_key(self):
        cursors = []
        def handler(request):
            self.assertEqual(request.headers["x-api-key"], "fixture-key")
            cursor = json.loads(request.content)["request"]["after_sequence"]
            cursors.append(cursor)
            return httpx.Response(200, json={"documents": [{"sequence": 7}] if cursor == 4 else [], "next_sequence": 7})
        async with AsyncMarketBridge(api_key="fixture-key", transport=httpx.MockTransport(handler)) as client:
            rows = [row async for row in client.documents("events", after_sequence=4)]
        self.assertEqual(rows, [{"sequence": 7}])
        self.assertEqual(cursors, [4, 7])

    async def test_redirect_not_followed_and_mutation_not_retried(self):
        calls = []
        def handler(request):
            calls.append(request)
            return httpx.Response(302, headers={"location": "https://other.test"})
        async with AsyncMarketBridge(transport=httpx.MockTransport(handler)) as client:
            with self.assertRaises(httpx.HTTPStatusError):
                await client.run("id", "model", {})
        self.assertEqual(len(calls), 1)

    async def test_cancellation_and_timeout(self):
        async def handler(request):
            await asyncio.sleep(10)
            return httpx.Response(200, json={})
        async with AsyncMarketBridge(timeout=0.02, transport=httpx.MockTransport(handler)) as client:
            with self.assertRaises(TimeoutError):
                await client.get("/v1/system/info")
            task = asyncio.create_task(client.get("/v1/system/info"))
            await asyncio.sleep(0)
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task

    async def test_nonfinite_input_and_invalid_cursor(self):
        async with AsyncMarketBridge() as client:
            with self.assertRaises(ValueError):
                await client.evaluate({"bad": float("nan")})
            with self.assertRaises(ValueError):
                await anext(client.documents("events", after_sequence=-1))

    async def test_bad_cursor_is_not_silently_skipped(self):
        async with AsyncMarketBridge(transport=httpx.MockTransport(lambda r: httpx.Response(200, json={"documents": [{"sequence": 1}], "next_sequence": 1}))) as client:
            with self.assertRaises(ValueError):
                await anext(client.documents("events", after_sequence=1))
