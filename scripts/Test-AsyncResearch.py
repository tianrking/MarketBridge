"""Real HTTP integration with a caller-started isolated research service."""
import argparse
import asyncio
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk" / "python"))
from marketbridge_async import AsyncMarketBridge

async def test(base_url):
    async with AsyncMarketBridge(base_url, api_key=os.environ.get("MARKETBRIDGE_API_KEY")) as client:
        info, control = await asyncio.gather(client.get("/v1/system/info"), client.get("/v1/research/control"))
        assert info["orders_supported"] is False and control["orders_supported"] is False
        ids = [doc["id"] async for doc in client.documents("runs")]
        assert "dataset-run" in ids and "failed-run" in ids
        integrity = await client.workspace("integrity")
        assert integrity["sqlite"] == "ok"
    print("PASS async real HTTP concurrent reads, cursor archive and integrity")

if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--base-url",default="http://127.0.0.1:8080")
    asyncio.run(test(parser.parse_args().base_url))
