# MarketBridge Documentation

This directory contains the English documentation set for MarketBridge. The
root-level `README.md` and `README.zh-CN.md` are the bilingual entry points; all
documents under `docs/` are maintained in English so release packages are easy
to browse and search consistently.

Current documented release: `v0.0.5`

Download the latest binary release:
[https://github.com/tianrking/MarketBridge/releases/latest](https://github.com/tianrking/MarketBridge/releases/latest)

All releases:
[https://github.com/tianrking/MarketBridge/releases](https://github.com/tianrking/MarketBridge/releases)

## Core Guides

| Document | Purpose |
|---|---|
| [`data_interfaces.md`](data_interfaces.md) | Main API contract, binary quick start, endpoint list, query parameters, and response-field notes. Start here when integrating a client. |
| [`usage_full.md`](usage_full.md) | End-to-end usage guide for running MarketBridge, querying current snapshots, discovering perpetual contracts, persisting selected data, and using websocket streams. |
| [`query_examples.md`](query_examples.md) | Large copy-paste query cookbook with 100+ examples for symbols, exchanges, funding, price change, basis, order flow, liquidations, exports, and operations. |
| [`data_sources.md`](data_sources.md) | Operator guide for source families, API-key requirements, CEX/perp coverage, DeFi, macro, sentiment, on-chain, and websocket source notes. |
| [`architecture.md`](architecture.md) | System architecture, event model, runtime boundaries, catalog endpoints, API surface, and extension guidelines. |

## Perpetual Markets And Funding

| Document | Purpose |
|---|---|
| [`query_examples.md`](query_examples.md) | Broad scenario cookbook covering common searches across funding, price change, exchange coverage, symbol discovery, exports, and monitoring setup. |
| [`perpetual_funding_cookbook.md`](perpetual_funding_cookbook.md) | Copy-paste `curl + jq` cookbook for perpetual contract discovery, funding-rate range searches, cross-exchange comparisons, CSV exports, and watchlist generation. |
| [`squeeze_and_exhaustion_examples.md`](squeeze_and_exhaustion_examples.md) | Read-only symbol-level squeeze and exhaustion-short monitoring examples, including current data dimensions and known gaps. |

## Coverage And Engineering References

| Document | Purpose |
|---|---|
| [`feature_inventory.md`](feature_inventory.md) | Current public API surfaces, exchange feature coverage, remaining gaps, and implementation status. |
| [`ccxt_parity_audit.md`](ccxt_parity_audit.md) | Reference audit for exchange connector parity and public-data coverage decisions. |
| [`source_expansion_inventory.md`](source_expansion_inventory.md) | External source expansion inventory and possible future connector families. |
| [`performance_review.md`](performance_review.md) | Performance review notes for the event bus, streaming, cache scans, serialization, and retention behavior. |

## Quick Binary Workflow

1. Download the latest package from
   [GitHub Releases](https://github.com/tianrking/MarketBridge/releases/latest).
2. Extract it.
3. Run the included binary with one of the included config files.
4. Call the local HTTP API from another terminal.

Linux/macOS example:

```bash
tar -xzf market-bridge-v0.0.5-linux-x86_64.tar.gz
cd market-bridge-v0.0.5-linux-x86_64
chmod +x ./market-bridge
MARKETBRIDGE_CONFIG=./config.yaml ./market-bridge
```

Smoke check:

```bash
curl -s http://127.0.0.1:8080/health | jq
curl -s http://127.0.0.1:8080/v1/system/info | jq
curl -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/search?q=HOME" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=20" | jq
```
