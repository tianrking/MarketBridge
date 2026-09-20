# MarketBridge Frontend

The research console is embedded in the Rust binary at `/workbench`; no Node.js
service or frontend build step is needed. See [the complete usage series](../docs/user-guide/README.md).
The older static funding monitor remains available separately as described below.

The workbench includes a read-only **Binance 做空研究** tab. It ranks the
currently observed squeeze candidates through `/v1/research/squeeze/scan`.
Concrete completed-candle entry/invalidation/1R/2R reference levels are
calculated by the Python scanner, because strategies remain Python-first:
`examples/crypto/microstructure/crypto_binance_short_opportunity_scanner.py`.

Run the API:

```bash
# From the repository root
MARKETBRIDGE_CONFIG=./config.min.yaml cargo run
```

Serve the frontend:

```bash
python3 -m http.server 8090 --bind 127.0.0.1 --directory frontend
```

Open:

```text
http://127.0.0.1:8090
```

For a VPS API:

```text
http://127.0.0.1:8090/?api=http://YOUR_VPS_IP:8080
```
