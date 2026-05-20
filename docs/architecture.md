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
  prediction/
    polymarket.rs
  onchain/
    ethereum.rs
    polygon.rs
  external/
    weather.rs
    news.rs
```

Connector output must be normalized into `DataEnvelope<T>` before it reaches the
shared cache or stream layer.

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
  routes/
    catalog.rs
    market.rs
    options.rs
    prediction.rs
    onchain.rs
    external.rs
    stream.rs
```

The API layer should not know exchange-specific websocket details.

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
GET /v1/market/orderbooks
GET /v1/market/trades
GET /v1/market/funding
GET /v1/market/open-interest
```

Purpose: CEX and DEX market data by source, venue, asset, product, symbol, and
instrument.

### Options Data

```text
GET /v1/options/chains
GET /v1/options/iv
GET /v1/options/instruments
```

Purpose: Deribit and future options venues.

### Prediction Market Data

```text
GET /v1/prediction/markets
GET /v1/prediction/books
GET /v1/prediction/trades
GET /v1/prediction/resolutions
```

Purpose: Polymarket, Kalshi-like public data, and future prediction-market
connectors.

### On-chain Data

```text
GET /v1/onchain/transfers
GET /v1/onchain/wallets
GET /v1/onchain/dex/pools
GET /v1/onchain/oracles
```

Purpose: wallet flows, DEX state, settlement/oracle references, and public chain
events.

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

## Current State

Implemented today:

- Catalog discovery through `GET /v1/catalog/sources`,
  `GET /v1/catalog/domains`, and `GET /v1/catalog/instruments`.
- Envelope-based market quote streaming through `WS /v1/stream`.
- Exchange spot/perp BBO and selected funding fields through legacy endpoints.
- Envelope-based exchange quote snapshots through `GET /v1/market/quotes`.
- Deribit option summary direct REST and background cache.
- Envelope-based Deribit option chains through `GET /v1/options/chains`.
- Polymarket crypto market discovery, REST books, and live CLOB cache.
- Envelope-based Polymarket books through `GET /v1/prediction/books`.
- Freshness fields for exchange ticks, Deribit cache rows, and Polymarket live books.

Known architecture gaps:

- `DataEvent` only supports `MarketTick`.
- `EventBus` only stores quote snapshots.
- `api.rs` contains too many route domains in one file.
- `external.rs` mixes unrelated source clients.
- Legacy endpoints are source-specific rather than `/v1` domain APIs.

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

Status: partially implemented. `domains/market/quote.rs` and
`GET /v1/market/quotes` exist. `WS /v1/stream` is still pending.

### Phase 3: Options and Prediction Domains

- Move Deribit cache payload into `domains/options/chain.rs`.
- Move Polymarket book payload into `domains/prediction/book.rs`.
- Add `/v1/options/chains` and `/v1/prediction/books`.

Status: implemented for first-pass Deribit chains and Polymarket books.

### Phase 4: Catalog and Source Registry

- Add a source registry that describes enabled connectors, domains, symbols,
  instruments, and cache health.
- Add `/v1/catalog/sources` and `/v1/catalog/instruments`.

Status: partially implemented. `/v1/catalog/sources`, `/v1/catalog/domains`,
`/v1/catalog/instruments`, and a first `market_quote` version of `WS /v1/stream`
exist. A runtime source-health registry and non-quote stream domains are still
pending.

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
