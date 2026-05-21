# MarketBridge Architecture

MarketBridge is an independent public data-source bridge. It does not decide
alpha, place orders, or run strategy logic. Its job is to connect public data
sources, normalize them, cache fresh state, and expose one stable API surface to
research systems such as PolyAlpha.

## Product Boundary

MarketBridge owns:

- Public exchange data: spot, perps, order books, trades, funding, open interest.
- Options data: chains, IV, greeks where public data is available.
- Prediction-market data: market metadata, CLOB books, trades, resolution data.
- On-chain data: blocks, transfers, wallet flows, DEX pools, oracle feeds.
- External event data: weather, news, sports, macro, official settlement data.
- Freshness, latency, stale flags, coverage, source health, and stream delivery.

MarketBridge does not own:

- Factor approval.
- Strategy decisions.
- Paper/live PnL attribution.
- Wallet signing, authenticated trading, order placement, or execution routing.

## Distribution

Version `v0.0.2` is published as a standalone binary package. The package
contains the `market-bridge` binary, example config files, `README.md`, `docs/`,
and a `VERSION` file.

Release automation lives in `.github/workflows/release.yml`:

- pushes to `main` or `master` build downloadable workflow artifacts;
- tag pushes such as `v0.0.2` build the same artifacts and attach them to a
  GitHub Release;
- supported packages are Linux x86_64, Linux i686, macOS x86_64, macOS aarch64,
  and Windows x86_64.

Runtime configuration remains file-based through `MARKETBRIDGE_CONFIG`; the
same config files work for both `cargo run` and downloaded binaries.
Code-level configuration lives under `src/config/`:

- `mod.rs`: top-level `AppConfig`, file loading, symbol normalization, and
  exchange lookup helpers.
- `runtime.rs`: runtime, API, Redis, broadcast, and backpressure settings.
- `strategy.rs`: strategy thresholds and fee-mode selection.
- `fees.rs`: exchange fee models and tier selection.
- `klines.rs`: OHLCV storage/backfill settings.
- `onchain.rs`: Whale Alert, mempool.space, and Etherscan settings.

## Layering

MarketBridge uses three layers. The names matter because each layer has a
different reason to change.

### 1. Connector Layer

Organized by source because every venue has different protocols, symbols,
pagination, rate limits, and websocket behavior.

Target layout:

```text
src/connectors/
  cex/
    binance.rs
    okx.rs
    bybit.rs
  options/
    deribit.rs
    okx.rs
    bybit.rs
    binance.rs
  prediction/
    polymarket.rs
  defi/
    jupiter.rs
    raydium.rs
    uniswap_v3.rs
    paraswap.rs
    oneinch.rs
  tradfi/
    yahoo.rs
    fred.rs
  aggregate/
    coingecko.rs
    coinmarketcap.rs
    coinglass.rs
  sentiment/
    fear_greed.rs
    cryptopanic.rs
    santiment.rs
    lunarcrush.rs
  external/
    weather.rs
    news.rs
```

Connector output must be normalized into `DataEnvelope<T>` before it reaches the
shared cache or stream layer.

Status: CEX exchange adapters now live under `src/connectors/cex`.
Option venue REST client code lives under `src/connectors/options`.
Polymarket Gamma/CLOB REST client code lives under
`src/connectors/prediction/polymarket.rs`. Gamma market parsing lives in
`src/connectors/prediction/polymarket_parser.rs` so parser heuristics can evolve
without touching CLOB REST wrappers.
Reusable CEX websocket reconnect policy lives in `src/connectors/cex/ws.rs`;
new websocket adapters should wrap one-session loops with `run_reconnecting`
instead of open-coding retry loops.

### 2. Domain Layer

Organized by data type because downstream consumers query by what the data is,
not by how it was collected.

Target layout:

```text
src/domains/
  market/
    quote.rs
    orderbook.rs
    trade.rs
    funding.rs
  options/
    chain.rs
    iv.rs
  prediction/
    market.rs
    book.rs
    trade.rs
    resolution.rs
  onchain/
    transfer.rs
    wallet.rs
    dex.rs
  external/
    weather.rs
    news.rs
```

The domain layer defines payload schemas, query filters, and cache keys.

### 3. Interface Layer

Organized by user workflow. Interfaces must be stable and source-agnostic.

Target layout:

```text
src/api/
  streaming.rs
  routes/
    catalog.rs
    market.rs
    options.rs
    prediction.rs
    onchain.rs
    external_event.rs
    stream.rs
```

`src/api/routes/*` owns HTTP/WebSocket routing and connection lifecycle.
Reusable stream filtering, domain selection, and bounded websocket sending live
in `src/api/streaming.rs` so new delivery modes can share the same rules.

The API layer should not know exchange-specific websocket details.

Source/domain discovery lives in `src/catalog.rs`; API routes expose it but do
not own the registry. `GET /v1/catalog/sources` overlays the static source
inventory with runtime config and reports `enabled`, `available`, or
`enabled_missing_api_key` so users can see which data sources are actually
wired for the current process.

## Canonical Envelope

Every normalized record should eventually use:

```json
{
  "version": "v1",
  "domain": "market_quote",
  "source_ref": {
    "source_type": "exchange",
    "source": "binance",
    "venue": "binance",
    "chain": null,
    "protocol": null
  },
  "instrument_ref": {
    "asset_class": "crypto",
    "product_type": "spot",
    "instrument_id": "BTC-USDT-SPOT",
    "symbol": "BTCUSDT",
    "base": "BTC",
    "quote": "USDT",
    "market_id": null
  },
  "freshness": {
    "ts_source": 1779255869089,
    "ts_received": 1779255869120,
    "latency_ms": 31,
    "stale": false
  },
  "payload": {}
}
```

This gives every data type the same operational controls: source, instrument,
time, latency, stale, and payload.

## Canonical Query Dimensions

All endpoints should converge toward these filters:

- `domain`
- `source_type`
- `source`
- `venue`
- `asset_class`
- `product_type`
- `symbol`
- `instrument_id`
- `base`
- `quote`
- `market_id`
- `chain`
- `protocol`
- `wallet`
- `include_stale`

Domain-specific filters are allowed, for example `strike_min`, `expiry_before`,
or `token_ids`, but they should be additive rather than replacing the common
dimensions.

## Target API Surface

Current endpoints can remain during migration. New endpoints should use `/v1`.

### Catalog

```text
GET /v1/catalog/sources
GET /v1/catalog/instruments
GET /v1/catalog/domains
```

Purpose: discover what MarketBridge can provide right now.

### Market Data

```text
GET /v1/market/quotes
GET /v1/market/order-books
GET /v1/market/trades
GET /v1/market/funding
GET /v1/market/open-interest
GET /v1/market/liquidations
```

Purpose: CEX and DEX market data by source, venue, asset, product, symbol, and
instrument.

Current public connector coverage is tracked in
[`feature_inventory.md`](feature_inventory.md). That matrix is the source of
truth for which venues expose BBO, L2 books, trades, funding, OI, and
liquidations.

### Options Data

```text
GET /v1/options/chains
GET /options/deribit/book
GET /options/okx/book
GET /options/bybit/book
GET /options/binance/book
```

Purpose: Deribit, OKX, Bybit, and Binance option venues. REST chain and
per-instrument depth are wired; public websocket ticker/summary cache updates
are wired for all four venues. Native websocket option book/trade streams remain
a latency extension tracked in the feature inventory.

### Prediction Market Data

```text
GET /v1/prediction/markets
GET /v1/prediction/books
GET /v1/prediction/trades
GET /v1/prediction/resolutions
```

Purpose: Polymarket, Kalshi-like public data, and future prediction-market
connectors.

### DeFi / On-chain Price Data

```text
GET /v1/market/quotes?exchanges=jupiter
GET /v1/market/quotes?exchanges=raydium
GET /v1/market/quotes?exchanges=uniswap_v3
GET /v1/market/quotes?exchanges=paraswap
GET /v1/market/quotes?exchanges=oneinch
```

Purpose: normalize DEX aggregator quotes and AMM pool prices into the same
`market_quote` domain used by CEX sources. Wallet transfers, raw RPC event
indexing, native pool liquidity state, and raw swap/trade feeds are future
domains.

### Traditional Finance Reference Data

```text
GET /v1/market/quotes?exchanges=dxy
GET /v1/market/quotes?exchanges=vix
GET /v1/market/quotes?exchanges=us10y
```

Purpose: normalize macro references such as DXY, VIX, and US10Y into the same
`market_quote` domain for regime filters and cross-asset context.

### Aggregate And Sentiment Data

```text
GET /v1/market/quotes?exchanges=coingecko,coinmarketcap
GET /v1/external/signals?sources=coinglass,fear_greed,cryptopanic,santiment,lunarcrush
```

Purpose: normalize aggregate derivatives data, global price references, news,
and social metrics into stable quote/signal surfaces. API-key-backed sources
read keys from config first and environment variables second.

### External Event Data

```text
GET /v1/external/weather
GET /v1/external/news
GET /v1/external/sports
GET /v1/external/macro
```

Purpose: event data that prediction-market strategies need for settlement and
nowcast validation.

### Unified Stream

```text
WS /v1/stream?domains=market_quote,prediction_book&symbols=BTCUSDT&include_stale=false
```

Purpose: one websocket stream for all normalized domains.

`/v1/stream` uses domain-specific broadcast channels for high-volume live
domains such as `funding`, `trade`, and `order_book`. Slow clients skip their
own buffered messages or are disconnected on send timeout; they do not block
publishers or other subscribers.

## Runtime State

MarketBridge keeps current-state snapshots in `src/event_snapshots.rs` and
event fanout in `src/event_bus.rs`.

- `event_snapshots.rs`: latest quote/funding/OI/trade/liquidation/book/external
  signal rows, cache key rules, and ArcSwap copy-on-write snapshot maps.
- `event_bus.rs`: raw event broadcast, per-domain broadcast channels, and
  update orchestration.
- `router.rs`: source fanout into the spread aggregator and an asynchronous
  bus worker so snapshot publication does not sit on the router hot path.
- `redis_sink.rs`: optional Redis Stream persistence with batched `XADD`
  pipelines and local JSONL dead-letter fallback at
  `runtime.redis_dead_letter_path`; the service remains a live data bridge when
  Redis is disabled.
- `klines.rs`: SQLite-backed OHLCV storage, historical Binance/OKX REST
  backfill, and realtime tick-to-candle aggregation.

This split is deliberate: future optimizations such as pre-serialized bytes or
sharded stores should stay in the runtime state layer without changing
connector code or API route logic.

## Strategy Signal Reporting

MarketBridge does not own strategy decisions, but it does emit operational
spread signals for data-plane sanity checks. Async aggregation and reporting
live in `src/aggregator.rs`; reusable spread math, executable depth pricing,
symbol normalization, and profit breakdowns live in `src/aggregator_signal.rs`.
This keeps deterministic signal math testable without starting source runtimes.

## Current State

Implemented today:

- Catalog discovery through `GET /v1/catalog/sources`,
  `GET /v1/catalog/domains`, `GET /v1/catalog/instruments`, and
  `GET /v1/catalog/health`.
- Domain-filtered market quote, funding, OI, trade, liquidation, order-book,
  external signal, options chain, and prediction book streaming through
  `WS /v1/stream`.
- Exchange spot/perp BBO and selected funding fields through legacy endpoints.
- Envelope-based exchange quote snapshots through `GET /v1/market/quotes`.
- DeFi aggregator quote and AMM pool snapshots through `GET /v1/market/quotes`.
- TradFi DXY, VIX, and US10Y snapshots through `GET /v1/market/quotes`.
- Aggregate and sentiment signals through `GET /v1/external/signals`.
- Deribit option summary direct REST and multi-venue option background cache.
- Envelope-based Deribit/OKX/Bybit/Binance option chains through `GET /v1/options/chains`.
- Per-instrument Deribit/OKX/Bybit/Binance option depth endpoints.
- Polymarket crypto market discovery, REST books, and live CLOB cache.
- Envelope-based Polymarket books through `GET /v1/prediction/books`.
- Freshness fields for exchange ticks, option cache rows, and Polymarket live books.

Known architecture gaps:

- Legacy endpoints are source-specific rather than `/v1` domain APIs.
- Options websocket book/trade parity is a latency extension: REST chain/depth
  and public websocket ticker/summary cache updates are wired.
- Runtime source health is cache-derived; deeper connector lifecycle telemetry
  is still a future enhancement.
- Remaining non-Polymarket data gaps are centralized in
  `feature_inventory.md#remaining-non-polymarket-data-gaps`.

## Migration Plan

### Phase 1: Foundation

- Keep current endpoints working.
- Add `core/schema.rs` with `DataEnvelope`, `SourceRef`, `InstrumentRef`, and `Freshness`.
- Add this architecture contract.

### Phase 2: Market Quotes

- Introduce `domains/market/quote.rs`.
- Convert existing exchange ticks into `DataEnvelope<QuotePayload>`.
- Add `GET /v1/market/quotes`.
- Add `WS /v1/stream` support for `market_quote`.

Status: implemented for first-pass exchange quote snapshots and streaming.

### Phase 3: Options and Prediction Domains

- Move Deribit cache payload into `domains/options/chain.rs`.
- Move Polymarket book payload into `domains/prediction/book.rs`.
- Add `/v1/options/chains` and `/v1/prediction/books`.

Status: implemented for multi-venue option chain snapshots, per-instrument
option books, and Polymarket books.

### Phase 4: Catalog and Source Registry

- Add a source registry that describes enabled connectors, domains, symbols,
  instruments, and cache health.
- Add `/v1/catalog/sources` and `/v1/catalog/instruments`.

Status: implemented as a first-pass catalog with source/domain/instrument
discovery and cache-derived health through `/v1/catalog/health`.

### Phase 5: On-chain and External Event Data

- Add connector namespaces for on-chain and event sources.
- Require every new source to produce `DataEnvelope<T>` and expose freshness.

## Design Rules

- Keep connectors source-specific.
- Keep domain models source-agnostic.
- Every cache row must expose `freshness`.
- Every query should support `include_stale`.
- Every domain should have both snapshot and stream paths where practical.
- Strategy logic belongs in PolyAlpha, not MarketBridge.
