# MarketBridge

Independent Rust data-source bridge for exchange, options, prediction-market, and external market data aggregation. MarketBridge normalizes public data, caches it, marks freshness, and exposes one API surface for downstream research systems.

![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust)
![Tokio](https://img.shields.io/badge/Runtime-Tokio-333333?logo=rust)
![Axum](https://img.shields.io/badge/Web-Axum-0ea5e9)
![WebSocket](https://img.shields.io/badge/Transport-WebSocket-2563eb)
![Redis](https://img.shields.io/badge/Stream-Redis-e11d48?logo=redis)
![Prometheus](https://img.shields.io/badge/Metrics-Prometheus-f97316?logo=prometheus)
![Serde](https://img.shields.io/badge/Serialization-Serde-16a34a)
![License](https://img.shields.io/badge/License-MIT-64748b)

## Table of Contents

- [Why This Project](#why-this-project)
- [Architecture Contract](#architecture-contract)
- [Tech Stack](#tech-stack)
- [Architecture](#architecture)
- [Runtime Pipeline](#runtime-pipeline)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Implemented Data Plane](#implemented-data-plane)
- [Strategy Readiness Matrix](#strategy-readiness-matrix)
- [API Overview](#api-overview)
- [API Details](#api-details)
- [Connection Model Matrix](#connection-model-matrix)
- [Bring-Up Guide](#bring-up-guide)
- [Testing](#testing)
- [Extend New Exchange](#extend-new-exchange)

## Why This Project

`MarketBridge` solves three hard problems for quant research teams:

- Unified market model across multiple exchanges and both `spot` / `perp`
- Unified API layer (`REST + WebSocket + Redis`) for downstream strategy systems
- Data quality visibility (funding coverage, stale ratio, latency percentiles, health status, alerts)

## Architecture Contract

MarketBridge is being standardized around a source-agnostic data envelope:

```text
connector source -> domain payload -> DataEnvelope -> cache/stream/API
```

The long-term architecture and `/v1` API contract are maintained in
[docs/architecture.md](docs/architecture.md). Current endpoints remain supported
while existing exchange, Deribit, and Polymarket data is migrated into the new
domain model.

## Tech Stack

- Language: `Rust 2024`
- Runtime: `Tokio`
- HTTP/WS API: `Axum`
- WS clients: `tokio-tungstenite`
- Serialization: `serde`, `serde_json`, `serde_yaml`
- Metrics: `prometheus`
- Stream sink: `redis` (`XADD`)
- Logging: `tracing`, `tracing-subscriber`

## Architecture

```mermaid
flowchart LR
  subgraph S[Exchange Sources]
    S1[Binance/OKX/Bybit/... Spot]
    S2[Binance/OKX/Bybit/... Perp]
  end

  S --> RT[SourceRuntime\nReconnect + Backpressure]
  RT --> Q[mpsc Queue]
  Q --> R[EventRouter]

  R --> BUS[EventBus\nBroadcast + Snapshot Store]
  R --> AGG[SpreadAggregator]

  AGG --> LOG[Signal Logs\nFILTERED/HOLDING/TRIGGER]
  BUS --> API[Axum API\n/health /snapshot /funding /coverage /ws/ticks]
  BUS --> REDIS[Redis Sink\nXADD ticks:*]

  CFG[config.yaml] --> RT
  CFG --> AGG
  MET[Prometheus Metrics] --> API
```

## Runtime Pipeline

1. Exchange adapters subscribe to public market WS channels.
2. `SourceRuntime` supervises source tasks and reconnects with backoff.
3. `EventRouter` fans data to both `EventBus` and `SpreadAggregator`.
4. `EventBus` maintains latest snapshots and real-time broadcast stream.
5. `SpreadAggregator` computes cross-exchange opportunity signals with fee/slippage logic.
6. API/Redis expose normalized data to quant consumers.

## Quick Start

### 1) Run

```bash
MARKETBRIDGE_CONFIG=./config.yaml cargo run
```

Use full-exchange sample:

```bash
MARKETBRIDGE_CONFIG=./config.all-exchanges.example.yaml cargo run
```

### 2) Smoke check

```bash
curl -s http://127.0.0.1:8080/health
```

### 3) First data checks

```bash
curl -s "http://127.0.0.1:8080/snapshot?symbol=BTCUSDT" | jq
curl -s "http://127.0.0.1:8080/funding?symbols=BTCUSDT" | jq
curl -s "http://127.0.0.1:8080/coverage?market=perp&symbols=BTCUSDT" | jq
```

## Configuration

Default file: `config.yaml`

- `runtime.queue_capacity`: source->router channel capacity
- `runtime.backpressure`: `block` or `drop_newest`
- `runtime.report_interval_ms`: signal report interval
- `runtime.stale_ttl_ms`: stale threshold
- `runtime.api_addr`: API bind address
- `runtime.redis_url`: optional Redis sink
- `strategy.*`: min profit, hold, slippage model
- `symbols`: global spot symbols
- `perp_symbols`: global perp symbols
- `exchanges.<name>.enabled`: source switch
- `exchanges.<name>.symbols/perp_symbols`: per-exchange override
- `exchanges.<name>.fee`: fixed/tiered fee model

## Implemented Data Plane

This service is the unified data plane for downstream strategy engines such as
`PolyAlpha`. Strategy logic, factor validation, paper execution, and live order
management should stay outside this repo.

### Exchange Data

| Capability | Status | Interface | Notes |
|---|---:|---|---|
| Spot BBO | Implemented | `GET /snapshot`, `WS /ws/ticks` | Normalized `bid`, `ask`, `symbol`, `exchange`, `market=spot`. |
| Perp BBO | Implemented | `GET /snapshot`, `WS /ws/ticks` | Normalized `market=perp`; symbol mapping differs by venue internally. |
| Perp mark/funding | Implemented where venue provides it | `GET /funding`, `GET /coverage` | Coverage depends on exchange adapter support and live venue payloads. |
| Multi-exchange quality | Implemented | `GET /coverage` | Stale ratio, latency percentiles, funding coverage, alerts. |
| Redis stream sink | Implemented optional | `runtime.redis_url` | Emits normalized ticks to Redis streams when configured. |

### Option / IV Data

| Capability | Status | Interface | Notes |
|---|---:|---|---|
| Deribit option summaries | Implemented | `GET /options/deribit/summary?currency=BTC` | Direct REST fetch. Returns strike, expiry, bid/ask, mark price, `mark_iv`, underlying price. |
| Deribit option cache | Implemented first version | `GET /options/deribit/live-summary` | Background REST cache for BTC/ETH option chains with `received_at_ms` and `stale`. |
| Deribit websocket IV | Not implemented | N/A | REST cache is enough for first paper loop; websocket IV cache is future work if REST freshness is not enough. |

### Polymarket Data

| Capability | Status | Interface | Notes |
|---|---:|---|---|
| Active BTC/ETH binary market discovery | Implemented first version | `GET /polymarket/crypto-markets` | Parses `base_asset`, strike, direction, rule type, expiry, Yes/No token ids from Gamma. |
| Single outcome CLOB book | Implemented | `GET /polymarket/book?token_id=...` | Returns full book plus best bid/ask, spread, bid/ask depth. |
| Batch outcome CLOB books | Implemented | `GET /polymarket/books?token_ids=...` | Useful for Yes/No pair checks. |
| Crypto markets plus books | Implemented | `GET /polymarket/crypto-books` | Convenience endpoint for strategy engines. |
| Polymarket CLOB websocket cache | Implemented first version | `GET /polymarket/live-books`, `GET /polymarket/live-crypto-books` | Seeds from REST snapshots, subscribes public CLOB websocket updates, and exposes `stale` for strategy-side freshness gates. |
| Polymarket official SDK/CLI integration | Not implemented | N/A | Current implementation uses public REST endpoints. SDK/CLI integration is future work for authenticated execution and schema safety. |
| Live order placement / cancel / replace | Not implemented | N/A | Execution belongs in a later trading/execution layer, not in this data-plane pass. |

## Strategy Readiness Matrix

For the crypto binary fair-value / market-making strategy discussed with
`PolyAlpha`, the required inputs are:

| Strategy Input | Needed For | Status in `MarketBridge` | Current Interface |
|---|---|---:|---|
| BTC/ETH spot/perp bid/ask | Underlying price and basis | Implemented | `/snapshot`, `/ws/ticks` |
| Perp funding | Basis/funding sanity check | Implemented where supported | `/funding` |
| Deribit IV / option chain | Theoretical digital probability | Implemented REST and cache first versions | `/options/deribit/summary`, `/options/deribit/live-summary` |
| Polymarket market id / strike / expiry | Map event to option inputs | Implemented first version | `/polymarket/crypto-markets` |
| Polymarket Yes/No token ids | Subscribe/query executable prices | Implemented first version | `/polymarket/crypto-markets` |
| Polymarket Yes/No bid/ask/depth | Entry, exit, pair discount, capacity | Implemented REST and live cache first versions | `/polymarket/book`, `/polymarket/books`, `/polymarket/crypto-books`, `/polymarket/live-books`, `/polymarket/live-crypto-books` |
| Stale/latency health | Decision input hygiene | Implemented first version | Exchange ticks expose stale/latency; Polymarket live cache exposes `received_at_ms`, `source_latency_ms`, `source`, and `stale`. |
| Paper decision/PnL loop | Validate signal after 5 minutes | Not implemented here | Belongs in `PolyAlpha`. |
| Live execution | Real order submit/cancel/fills | Not implemented | Future execution layer; not approved for live trading. |

Bottom line: `MarketBridge` now provides a first mature data-source surface for
paper decisions: exchange BBO/funding, Deribit option summaries, Polymarket
market discovery, REST books, and a live Polymarket CLOB cache. It is still not
an execution engine: authenticated Polymarket order placement/cancel/replace and
strategy PnL validation belong in later layers.

## API Overview

Base URL: `http://127.0.0.1:8080`

| Method | Path | Purpose |
|---|---|---|
| GET | `/` | Service metadata |
| GET | `/health` | Liveness check |
| GET | `/v1/catalog/sources` | Implemented public data sources |
| GET | `/v1/catalog/domains` | Implemented normalized data domains |
| GET | `/v1/catalog/instruments` | Instruments currently visible in live caches |
| GET | `/v1/catalog/health` | Source/domain record counts and freshness status |
| GET | `/v1/market/quotes` | Envelope-based exchange spot/perp quote snapshots |
| GET | `/v1/market/funding` | Funding-rate snapshots from public perp feeds |
| GET | `/v1/market/open-interest` | Open-interest snapshots from public feeds/REST |
| GET | `/v1/market/liquidations` | Latest public liquidation events |
| GET | `/v1/market/order-books` | Latest L2 book snapshots |
| GET | `/v1/market/trades` | Latest public trade snapshots |
| GET | `/v1/options/chains` | Envelope-based cached Deribit option chains |
| GET | `/v1/prediction/books` | Envelope-based cached Polymarket CLOB books |
| GET | `/snapshot` | Latest normalized ticks |
| GET | `/funding` | Unified perp funding view |
| GET | `/options/deribit/summary` | Deribit option chain summaries and IV |
| GET | `/options/deribit/live-summary` | Cached Deribit option summaries with freshness fields |
| GET | `/polymarket/crypto-markets` | Parsed Polymarket BTC/ETH binary markets |
| GET | `/polymarket/book` | Polymarket CLOB book summary for one token |
| GET | `/polymarket/books` | Polymarket CLOB book summaries for token ids |
| GET | `/polymarket/crypto-books` | Parsed crypto markets plus Yes/No CLOB books |
| GET | `/polymarket/live-books` | Cached Polymarket CLOB books seeded by REST and patched by websocket |
| GET | `/polymarket/live-crypto-books` | Parsed crypto markets plus cached Yes/No CLOB books |
| GET | `/coverage` | Data quality dashboard model |
| GET | `/metrics` | Prometheus metrics text |
| WS | `/ws/ticks` | Real-time normalized tick stream |
| WS | `/v1/stream` | Envelope-based stream for `market_quote`, `options_chain`, and `prediction_book` |

### Exchange Public Data Coverage

| Venue | Funding | Open interest | Liquidations | L2 book | Trades |
|---|---|---|---|---|---|
| Binance | WS `markPrice@1s` | REST poll `openInterest` | WS `forceOrder` | WS `depth20@100ms` | WS `aggTrade` |
| Bybit | WS `tickers` | WS `tickers` | WS `allLiquidation` | WS `orderbook.50` | WS `publicTrade` |
| OKX | WS `funding-rate` | WS `open-interest` | REST poll `liquidation-orders` | WS `books5` | WS `trades` |
| Hyperliquid | WS `activeAssetCtx` | WS `activeAssetCtx` | Not exposed as a stable all-market public channel | WS `l2Book` | WS `trades` |
| dYdX v4 | REST poll `perpetualMarkets` | REST poll `perpetualMarkets` | Not exposed as a stable all-market public channel | WS `v4_orderbook` | WS `v4_trades` |
| Backpack | Product-dependent | Product-dependent | Not exposed as a stable all-market public channel | WS `depth` | WS `trade` |
| MEXC | Perp ticker when field is present | Not yet exposed | Not yet exposed | WS spot/futures depth | WS spot/futures deals |
| BingX | Swap ticker when field is present | Swap ticker when field is present | Not yet exposed | WS `depth20` | WS `trade` |

Other CEX adapters still provide BBO and venue-specific mark/funding fields where
their ticker feed includes them. The new typed feeds are wired first for
Binance, Bybit, and OKX because they cover the highest-volume public derivatives
venues and the exact sources needed for funding/OI/liquidation/depth/trade
research.

The newer venue connectors are public-data only and keyless. Where a venue does
not provide a stable all-market liquidation stream, MarketBridge leaves that
domain empty instead of fabricating a signal.

## API Details

### `GET /`

Returns service info.

Example:

```bash
curl -s http://127.0.0.1:8080/
```

### `GET /health`

Simple liveness endpoint.

Example:

```bash
curl -s http://127.0.0.1:8080/health
```

### `GET /v1/catalog/*`

Catalog endpoints for discovering what MarketBridge can provide right now.

Examples:

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/domains" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/instruments" | jq
```

### `GET /v1/market/quotes`

Envelope-based exchange quote snapshots. This is the first `/v1` domain endpoint
and should be preferred by new consumers.

Query params:

- `symbols=BTCUSDT,ETHUSDT`
- `exchanges=okx,bybit,bitget`
- `product_type=spot|perp`
- `include_stale=true|false`, default `false`

Examples:

```bash
curl -s "http://127.0.0.1:8080/v1/market/quotes?symbols=BTCUSDT&product_type=perp" | jq
```

### `GET /v1/options/chains`

Envelope-based option chain snapshots from the Deribit cache.

Query params:

- `currency`, optional, e.g. `BTC`
- `option_type`, optional, `call` or `put`
- `strike_min`, `strike_max`, optional numeric filters
- `expiry_after`, `expiry_before`, optional ISO timestamp string filters
- `include_stale=true|false`, default `false`

Example:

```bash
curl -s "http://127.0.0.1:8080/v1/options/chains?currency=BTC&option_type=call" | jq
```

### `GET /v1/prediction/books`

Envelope-based prediction-market order books from the Polymarket live cache.

Query params:

- `token_ids`, optional comma-separated Polymarket token ids
- `include_stale=true|false`, default `false`

Example:

```bash
curl -s "http://127.0.0.1:8080/v1/prediction/books?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### `GET /snapshot`

Returns in-memory latest snapshots from `EventBus`.

Query params:

- `symbol` optional, e.g. `BTCUSDT`

Examples:

```bash
curl -s http://127.0.0.1:8080/snapshot | jq
curl -s "http://127.0.0.1:8080/snapshot?symbol=BTCUSDT" | jq
```

Key fields in each item:

- `exchange`, `market`, `symbol`
- `bid`, `ask`, `mark`, `funding`
- `ts`, `source_latency_ms`, `stale`

### `WS /ws/ticks`

Normalized tick stream subscription.

Query params:

- `symbols=BTCUSDT,ETHUSDT`
- `exchanges=okx,bybit`
- `market=spot|perp`

Example:

```bash
wscat -c "ws://127.0.0.1:8080/ws/ticks?market=perp&symbols=BTCUSDT"
```

### `WS /v1/stream`

Envelope-based websocket stream. It supports live `market_quote` events and
cached snapshot streaming for `options_chain` and `prediction_book`.

Query params:

- `domains=market_quote,options_chain,prediction_book`
- `symbols=BTCUSDT`
- `exchanges=okx,deribit,polymarket`
- `product_type=spot|perp|option|binary_outcome`
- `include_stale=true|false` default `false`
- `snapshot_interval_ms=1000` for cached domains, clamped to `250..60000`

Example:

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=market_quote&symbols=BTCUSDT&product_type=perp"
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=options_chain,prediction_book&include_stale=false"
```

### `GET /funding`

Unified perp funding view by canonical symbol.

Query params:

- `symbols=BTCUSDT,ETHUSDT`
- `exchanges=okx,bybit,bitget`
- `only_with_funding=true|false` default `true`
- `include_stale=true|false` default `false`

Examples:

```bash
curl -s "http://127.0.0.1:8080/funding" | jq
curl -s "http://127.0.0.1:8080/funding?symbols=BTCUSDT&exchanges=okx,bybit,bitget" | jq
```

Response model per symbol:

- `symbol`
- `exchanges_total`, `exchanges_with_funding`
- `min_funding`, `max_funding`, `funding_spread`
- `updated_at`
- `points[]` with `exchange/raw_symbol/funding/mark/stale/source_latency_ms/ts`

### `GET /options/deribit/summary`

Unified Deribit option summary feed for crypto binary-pricing models.

Query params:

- `currency=BTC|ETH`, default `BTC`

Example:

```bash
curl -s "http://127.0.0.1:8080/options/deribit/summary?currency=BTC" | jq
```

Key fields in `summaries[]`:

- `instrument_name`, `option_type`, `strike`, `expiry_time`
- `bid_price`, `ask_price`, `mark_price`, `mark_iv`
- `underlying_price`, `underlying_index`, `open_interest`

### `GET /options/deribit/live-summary`

Cached Deribit option summary feed. This endpoint reads the in-process data
cache instead of hitting Deribit on every strategy decision.

Enable the background cache in config:

```yaml
deribit:
  enabled: true
  base_url: "https://www.deribit.com/api/v2/"
  currencies: [BTC, ETH]
  refresh_secs: 10
  stale_ttl_ms: 30000
```

Query params:

- `currency`, optional, e.g. `BTC`
- `option_type`, optional, `call` or `put`
- `strike_min`, `strike_max`, optional numeric filters
- `expiry_after`, `expiry_before`, optional ISO timestamp string filters
- `include_stale=true|false`, default `false`

Example:

```bash
curl -s "http://127.0.0.1:8080/options/deribit/live-summary?currency=BTC&option_type=call&strike_min=90000&strike_max=120000" | jq
```

Key fields in `summaries[]`:

- `source`: `deribit_rest_cache`
- `received_at_ms`, `stale`
- all direct Deribit summary fields from `/options/deribit/summary`

### `GET /polymarket/crypto-markets`

Parsed active Polymarket BTC/ETH binary markets for downstream strategy engines.

Query params:

- `limit`, default `500`
- `max_offset`, default `5000`
- `gamma_base_url`, default `https://gamma-api.polymarket.com/`

Example:

```bash
curl -s "http://127.0.0.1:8080/polymarket/crypto-markets?limit=500&max_offset=500" | jq
```

Response fields:

- `markets[]`: parsed `base_asset`, `strike`, `direction`, `rule_type`, `expiry_time`, Yes/No token ids
- `clob_asset_ids[]`: token ids that a Polymarket CLOB collector should subscribe to

### `GET /polymarket/book`

Polymarket CLOB book summary for one outcome token.

Query params:

- `token_id` required

Example:

```bash
curl -s "http://127.0.0.1:8080/polymarket/book?token_id=TOKEN_ID" | jq
```

Key fields in `book`:

- `asset_id`, `market`, `timestamp`
- `best_bid`, `best_ask`, `spread`
- `bid_depth`, `ask_depth`
- full raw `book.bids[]` and `book.asks[]`

### `GET /polymarket/books`

Batch Polymarket CLOB book summaries.

Query params:

- `token_ids` comma-separated token ids

Example:

```bash
curl -s "http://127.0.0.1:8080/polymarket/books?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### `GET /polymarket/crypto-books`

Convenience endpoint for strategy engines: parsed active BTC/ETH binary markets plus the current Yes/No CLOB book summaries.

Query params are the same as `/polymarket/crypto-markets`.

### `GET /polymarket/live-books`

Cached Polymarket CLOB books for outcome token ids. The cache is populated by
REST snapshots first, then patched by public Polymarket CLOB websocket events
when they arrive.

Enable the background cache in config:

```yaml
polymarket:
  enabled: true
  ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market"
  gamma_base_url: "https://gamma-api.polymarket.com/"
  limit: 500
  max_offset: 5000
  refresh_secs: 300
  ping_secs: 10
  chunk_size: 500
  stale_ttl_ms: 1500
```

Query params:

- `token_ids` optional comma-separated token ids. If omitted, returns all cached books.

Example:

```bash
curl -s "http://127.0.0.1:8080/polymarket/live-books?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

Key fields in `books[]`:

- `source`: `polymarket_clob_rest` for seed snapshots, `polymarket_clob_ws` for websocket updates
- `last_event_type`: `book`, `best_bid_ask`, or `price_change`
- `best_bid`, `best_ask`, `spread`, `bid_depth`, `ask_depth`
- `received_at_ms`, `source_latency_ms`, `stale`

Decision runners should reject a Polymarket book when `stale=true` or when the
source is not fresh enough for the intended holding period. This is deliberate:
the data plane exposes truth, the strategy layer decides whether to trade.

### `GET /polymarket/live-crypto-books`

Parsed active BTC/ETH binary markets plus cached Yes/No books from the live
Polymarket cache.

Query params are the same as `/polymarket/crypto-markets`.

Example:

```bash
curl -s "http://127.0.0.1:8080/polymarket/live-crypto-books?limit=500&max_offset=500" | jq
```

### `GET /coverage`

Dashboard-grade quality model with global summary, market summary, exchange summary, symbol detail, and alerts.

Query params:

- `symbols=BTCUSDT,ETHUSDT`
- `exchanges=okx,bybit,bitget,binance`
- `market=spot|perp`
- `include_stale=true|false` default `true`
- `only_with_funding=true|false` default `false`

Examples:

```bash
curl -s "http://127.0.0.1:8080/coverage" | jq
curl -s "http://127.0.0.1:8080/coverage?market=perp&symbols=BTCUSDT" | jq
curl -s "http://127.0.0.1:8080/coverage?market=perp&exchanges=okx,bybit,bitget" | jq
```

Top-level fields:

- `generated_at`
- `query` normalized effective filters
- `summary` global KPIs
- `summary.markets[]` market-level KPIs
- `exchange_summaries[]` per-exchange health profile
- `alerts[]` global/exchange/symbol alerts
- `symbols[]` symbol-level details

`summary` KPIs include:

- `total_symbols`, `total_points`
- `healthy_symbols`, `warning_symbols`, `critical_symbols`
- `stale_ratio`, `funding_coverage_ratio`
- `online_exchange_count`, `expected_exchange_count`, `exchange_online_ratio`
- `latency_ms_p50`, `latency_ms_p95`

`symbols[]` fields include:

- `symbol`, `market`, `health_status`, `alerts[]`
- `exchanges_total`, `exchanges_with_funding`, `funding_coverage_ratio`
- `exchanges_stale`, `stale_ratio`
- `latency_ms_min`, `latency_ms_p50`, `latency_ms_avg`, `latency_ms_p95`, `latency_ms_max`
- `points[]` per-exchange latest data

### `GET /metrics`

Prometheus text metrics endpoint.

Example:

```bash
curl -s http://127.0.0.1:8080/metrics
```

Current metrics include:

- `ticks_ingested_total`
- `bus_publish_total`
- `ws_subscribers`
- `redis_xadd_total`
- `ticks_dropped_total`

## Connection Model Matrix

| Exchange | Spot model | Perp model (this project) | Notes |
|---|---|---|---|
| Binance | Single WS combined stream | Single WS combined stream | Spot `stream.binance.com`, perp `fstream.binance.com` |
| OKX | Single WS multi-subscribe | Single WS multi-subscribe | `tickers` + `-SWAP` mapping |
| Bybit | Single WS multi-subscribe | Single WS multi-subscribe | v5 `spot` / `linear` |
| Bitget | Single WS multi-subscribe | Single WS multi-subscribe | v2 public WS |
| KuCoin | Single WS multi-topic | Single WS multi-topic | tokenized endpoint |
| Gate | Single WS multi-symbol | Single WS multi-symbol | separate spot/perp WS domains |
| Kraken | Single WS multi-symbol | Single WS multi-symbol | perp symbol naming is venue-specific |
| HTX | Single WS multi-channel | Single WS multi-channel | gzip payload |
| Bitfinex | Single WS multi-channel | Single WS multi-channel | `chanId -> symbol` map |
| Coinbase | Single WS multi-product | Not implemented | spot only in this project |

Perp adapters enabled in code:

- `okx_perp`, `bybit_perp`, `bitget_perp`, `binance_perp`, `kucoin_perp`, `gate_perp`, `kraken_perp`, `htx_perp`, `bitfinex_perp`

Perp symbol conversion defaults:

- Binance / Bybit / Bitget: `BTCUSDT`
- OKX / HTX: `BTC-USDT-SWAP` (OKX) / `BTC-USDT` (HTX)
- KuCoin Perp: `BTCUSDTM`
- Gate Perp: `BTC_USDT`
- Bitfinex Perp: `tBTCF0:USDTF0`
- Kraken Perp: pass-through (configure exact venue symbol)

## Bring-Up Guide

1. Start with one spot exchange (`binance` or `okx`) and verify steady snapshot updates.
2. Enable one perp exchange and verify `market=perp` plus `mark/funding` fields.
3. Enable two perp exchanges and verify `/funding` spread and `/coverage` health split.
4. Expand to full config and monitor `/coverage` + `/metrics` continuously.

Practical notes:

- If an exchange is region-restricted, keep `enabled: false`.
- Kraken perp symbol naming is pass-through in this repo.
- Coinbase is spot-only in this repo.

## Testing

Run checks:

```bash
cargo fmt
cargo check
cargo test
```

## Extend New Exchange

1. Add `src/connectors/cex/<name>.rs`
2. Implement `ExchangeSource`
3. Convert payloads into `MarketTick` (`Spot` or `Perp`)
4. Register source in `src/connectors/cex/registry.rs`
