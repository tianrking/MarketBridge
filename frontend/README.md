# MarketBridge Frontend

The research console is embedded in the Rust binary at `/workbench`; no Node.js
service or frontend build step is needed. See [the complete usage series](../docs/user-guide/README.md).
The older static funding monitor remains available separately as described below.

The workbench includes a read-only **Binance 做空研究** tab. It ranks the
currently observed squeeze candidates through `/v1/research/squeeze/scan`.
Concrete completed-candle entry/invalidation/1R/2R reference levels are
calculated by the Python scanner, because strategies remain Python-first:
`examples/crypto/microstructure/crypto_binance_short_opportunity_scanner.py`.
The **标的事实指标** tab queries `/v1/research/symbol-state` for one selected
symbol and can refresh it every five seconds; no browser action can place an
order.

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

## Continuous Binance research scanner

Keep the Rust MarketBridge process running, then start the Python scanner with
`--iterations 0`. It prints every scan, appends only newly eligible candidates
to a local JSONL file, and optionally POSTs the same research payload to a
private webhook:

```bash
python3 examples/crypto/microstructure/crypto_binance_short_opportunity_scanner.py \
  --base-url http://127.0.0.1:8080 \
  --minimum-score 5 --limit 100 \
  --candle-workers 8 --interval-secs 30 --iterations 0 \
  --signal-file work/binance-short-opportunities.jsonl
```

The default configuration is deliberately bounded. Add reviewed Binance
perpetual symbols to `perp_symbols` before expanding the scan; use a process
supervisor (systemd, launchd, or Docker restart policy) for unattended uptime.

For a VPS API:

```text
http://127.0.0.1:8090/?api=http://YOUR_VPS_IP:8080
```
