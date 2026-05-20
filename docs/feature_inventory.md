# MarketBridge Feature Inventory

This document is the operating checklist for MarketBridge coverage. Update it in
the same commit as every connector, API, or infrastructure change.

Broader CCXT/Hummingbot source coverage candidates are tracked in
[`source_expansion_inventory.md`](source_expansion_inventory.md). That file is a
roadmap inventory only; this document remains the source of truth for what is
implemented and wired into the runtime/API.

Status labels:

- `implemented`: code path exists and is wired into runtime/API.
- `partial`: some fields or venues exist, but coverage is incomplete.
- `planned`: not implemented yet.
- `keyed`: requires user API key in config or environment variable.
- `keyless`: public endpoint without API key.

## Public API Surfaces

| Domain | Endpoint | Status | API key | Notes |
|---|---|---:|---:|---|
| Market quotes | `/v1/market/quotes` | implemented | mixed | CEX, DeFi, TradFi, CoinGecko, CoinMarketCap. |
| Source roadmap | `/v1/catalog/source-roadmap` | implemented | keyless | Hummingbot/CCXT expansion inventory; not runtime data coverage. |
| Spot-perp basis | `/v1/market/basis` | implemented | keyless | Derived from current spot/perp quote snapshots per exchange/symbol. |
| Funding rates | `/v1/market/funding` | implemented | mixed | Native CEX feeds plus future aggregate feeds. |
| Open interest | `/v1/market/open-interest` | implemented | keyless | Native CEX feeds plus future aggregate feeds. |
| Liquidations | `/v1/market/liquidations` | implemented | keyless | Native venue feeds where available. |
| Order books | `/v1/market/order-books` | implemented | mixed | Latest L2 snapshot per venue/symbol. |
| Trades | `/v1/market/trades` | implemented | mixed | Latest trade per venue/symbol. |
| Order flow | `/v1/market/order-flow` | implemented | keyless | Derived buy/sell volume, delta, notional, CVD, and large-trade count from live trades. |
| Klines | `/v1/market/klines` | implemented | keyless | SQLite-backed historical/rest and realtime tick-aggregated OHLCV bars. |
| Options chains | `/v1/options/chains` | implemented | keyless | Deribit/OKX/Bybit/Binance REST cache. |
| Prediction books | `/v1/prediction/books` | implemented | keyless | Polymarket live CLOB cache. |
| External signals | `/v1/external/signals` | implemented | mixed | CoinGlass, news, social, sentiment. |
| Onchain transfers | `/v1/onchain/transfers` | implemented | mixed | Whale Alert, mempool.space, and Etherscan large-transfer collectors. |

## CEX Connector Coverage

| Venue | BBO | L2 book | Trades | Funding | OI | Liquidations | API key | Notes |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| Binance | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Spot/perp public feeds. |
| Bybit | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Spot/perp public feeds. |
| OKX | implemented | implemented | implemented | implemented | implemented | implemented | keyless | REST liquidation poller. |
| Hyperliquid | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp DEX source. |
| dYdX v4 | partial | implemented | implemented | implemented | implemented | planned | keyless | REST market metadata plus WS book/trades. |
| Backpack | partial | implemented | implemented | planned | planned | planned | keyless | Product dependent public fields. |
| MEXC | partial | implemented | implemented | partial | planned | planned | keyless | Spot/futures depth/deals; funding when ticker field exists. |
| BingX | partial | implemented | implemented | partial | partial | planned | keyless | Swap ticker/depth/trade. |
| Bitget | implemented | implemented | implemented | implemented | implemented | planned | keyless | Public WS ticker/books5/trade; perp ticker also emits funding and OI. |
| Bitmart | partial | implemented | implemented | partial | planned | planned | keyless | Public spot depth/trades and perp depth/trades/funding/ticker. |
| Bitstamp | partial | implemented | implemented | planned | planned | planned | keyless | Public spot order-book diffs and trades. |
| Bitrue | partial | implemented | planned | planned | planned | planned | keyless | Public spot simple-depth stream from Hummingbot connector. |
| AscendEX | partial | implemented | implemented | planned | planned | planned | keyless | Public spot order-book diffs and trades from Hummingbot WS endpoint. |
| BTC Markets | partial | implemented | implemented | planned | planned | planned | keyless | Public spot order book and trades. |
| Aevo | partial | implemented | implemented | implemented | planned | planned | keyless | Public perp order books/trades plus REST funding and instrument BBO. |
| Pacifica | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp order books/trades/prices with funding and OI. |
| GRVT | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp book/trade/ticker streams with funding and OI. |
| Vertex | partial | implemented | implemented | planned | planned | planned | keyless | Public CLOB spot/perp book-depth and trade streams for known product ids. |
| Injective | partial | implemented | implemented | implemented | planned | planned | keyless | Public LCD/Sentry spot/perp order books and trades plus perp funding poller. |
| XRPL | partial | implemented | planned | planned | planned | planned | keyless | Public book_offers snapshots; issuer-aware mapping currently includes XRP/USD. |
| Architect | partial | implemented | implemented | implemented | planned | planned | keyed | Read-only perp WS book/trade and REST funding; requires bearer token. |
| Decibel | partial | implemented | implemented | implemented | planned | planned | keyed | Read-only Aptos Decibel perp depth/trades/market_price streams; requires bearer token and market address discovery. |
| Evedex | partial | implemented | implemented | implemented | planned | planned | keyless | Public perp REST depth, recent trades, and instrument funding metrics. |
| Derive | partial | implemented | implemented | implemented | planned | planned | keyless | Public spot/perp order books and trades plus perp ticker funding. |
| Dexalot | partial | implemented | implemented | planned | planned | planned | keyless | Public CLOB DEX spot order books and trades. |
| KuCoin | implemented | planned | planned | planned | planned | planned | keyless | Spot/perp ticker today. |
| Gate | implemented | planned | planned | planned | planned | planned | keyless | BookTicker-style BBO today. |
| Kraken | implemented | planned | planned | planned | planned | planned | keyless | Spot V2 ticker and separate futures ticker today. |
| HTX | implemented | planned | planned | planned | planned | planned | keyless | BBO only today. |
| Bitfinex | implemented | planned | planned | planned | planned | planned | keyless | Spot/perp ticker today. |
| Coinbase | implemented | planned | planned | n/a | n/a | n/a | keyless | Spot ticker only; Coinbase International not wired. |
| Crypto.com | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST ticker/book/trades for spot/perp plus perp funding and OI. |
| Cube | partial | implemented | planned | planned | planned | planned | keyless | Public REST MBP snapshots with market metadata scaling; protobuf WS diffs/trades remain planned. |
| Foxbit | partial | implemented | planned | planned | planned | planned | keyless | Public Brazil spot REST order-book snapshots; WS trades remain planned. |
| NDAX | partial | implemented | planned | planned | planned | planned | keyless | Public Canadian spot L2 snapshots via GetInstruments/GetL2Snapshot. |

## DeFi Connector Coverage

| Source | Quote | Pool/book snapshot | Trades | API key | Notes |
|---|---:|---:|---:|---:|---|
| Jupiter | implemented | planned | planned | keyless | Solana aggregator quote API. |
| Meteora | implemented | partial | planned | keyless | DexScreener-backed Solana pool quote snapshots for Gateway coverage. |
| Orca | implemented | partial | planned | keyless | DexScreener-backed Solana pool quote snapshots for Gateway coverage. |
| Raydium | implemented | partial | planned | keyless | Raydium token price map. |
| Uniswap V3 | implemented | partial | planned | keyless | Subgraph pool price snapshot. |
| ParaSwap | implemented | planned | planned | keyless | EVM aggregator quote API. |
| 1inch | implemented | planned | planned | keyless | EVM aggregator quote API. |
| PancakeSwap | implemented | partial | planned | keyless | DexScreener-backed BSC pool quote snapshots for Gateway coverage. |
| Balancer | implemented | partial | planned | keyless | DexScreener-backed Ethereum pool quote snapshots for Gateway coverage. |
| Curve | implemented | partial | planned | keyless | DexScreener-backed Ethereum stable-pool quote snapshots for Gateway coverage. |
| SushiSwap | implemented | partial | planned | keyless | DexScreener-backed Ethereum pool quote snapshots for Gateway coverage. |
| QuickSwap | implemented | partial | planned | keyless | DexScreener-backed Polygon pool quote snapshots for Gateway coverage. |
| Trader Joe | implemented | partial | planned | keyless | DexScreener-backed Avalanche pool quote snapshots for Gateway coverage. |
| ETCSwap | implemented | partial | planned | keyless | DexScreener-backed Ethereum Classic pool quote snapshots for Gateway coverage. |

## Polymarket Coverage

| Capability | Status | API key | Notes |
|---|---:|---:|---|
| Gamma market discovery | implemented | keyless | BTC/ETH crypto binary parser today. |
| REST book | implemented | keyless | `/polymarket/book`, `/polymarket/books`. |
| Live CLOB cache | implemented | keyless | `/polymarket/live-books`, `/v1/prediction/books`. |
| Midpoint batch | implemented | keyless | `/polymarket/midpoints` wraps CLOB `POST /midpoints`, max 500 token ids. |
| Last trade price batch | implemented | keyless | `/polymarket/last-trade-prices` wraps CLOB `POST /last-trades-prices`, max 500 token ids. |
| Spread batch | implemented | keyless | `/polymarket/spreads` wraps CLOB `POST /spreads`, max 500 token ids. |
| Market prices batch | implemented | keyless | `/polymarket/prices` wraps CLOB `POST /prices`, default sides `BUY,SELL`. |
| Historical prices/OHLCV | implemented | keyless | `/polymarket/prices-history` wraps CLOB `GET /prices-history` and `POST /batch-prices-history`, batch max 20 token ids. |
| Full category coverage | planned | keyless | Politics/sports/tech/general parser needed. |
| Open interest/live volume | planned | keyless | Endpoint support needs confirmation per market. |

## Options Coverage

| Venue | Summary chain | Greeks | WS ticker/book/trades | Depth | API key | Notes |
|---|---:|---:|---:|---:|---:|---|
| Deribit | implemented | partial | planned | planned | keyless | Summary currently omits greeks if not returned. |
| OKX Options | implemented | partial | planned | planned | keyless | REST option summary. |
| Bybit Options | implemented | implemented | planned | planned | keyless | Ticker contains greeks and IV fields in public payload. |
| Binance Options | implemented | partial | planned | planned | keyless | Ticker plus mark endpoint. |

## Aggregate, Macro, Sentiment Coverage

| Source | Domain | Status | API key | Env var |
|---|---|---:|---:|---|
| CoinGecko | market_quote | implemented | optional | `COINGECKO_API_KEY` |
| CoinCap | market_quote | implemented | optional | `COINCAP_API_KEY` |
| CoinMarketCap | market_quote | implemented | required | `COINMARKETCAP_API_KEY` |
| CoinGlass | external_signal | implemented | required | `COINGLASS_API_KEY` |
| Custom API | external_signal | implemented | keyless | Generic numeric/JSON pollers configured by `aggregates.custom_apis`. |
| Fear & Greed | external_signal | implemented | keyless | n/a |
| CryptoPanic | external_signal | implemented | required | `CRYPTOPANIC_API_KEY` |
| Santiment | external_signal | implemented | required | `SANTIMENT_API_KEY` |
| LunarCrush | external_signal | implemented | required | `LUNARCRUSH_API_KEY` |
| DXY | market_quote | implemented | keyless | n/a |
| VIX | market_quote | implemented | keyless | n/a |
| US10Y | market_quote | implemented | required | `FRED_API_KEY` |

## Onchain Transfer Coverage

| Source | Chain | Status | API key | Notes |
|---|---|---:|---:|---|
| Whale Alert | multi-chain | implemented | required | Global large transfer feed; configured by `WHALE_ALERT_API_KEY` and `min_value_usd`. |
| mempool.space | Bitcoin | implemented | keyless | Recent mempool transaction poller; filters by `min_value_btc` when payload exposes value. |
| Etherscan | Ethereum | implemented | required | Address watchlist transfer poller; configured by `ETHERSCAN_API_KEY` and `onchain.etherscan.addresses`. |

## Infrastructure Gaps

| Capability | Status | Priority | Notes |
|---|---:|---:|---|
| Redis all event types | implemented | P0 | Writes quote/funding/OI/trade/book/liquidation/external_signal/heartbeat streams. |
| Redis batch pipeline | implemented | P0 | Optional sink batches up to 100 events or 50ms per Redis `XADD` pipeline before retry/dead-letter accounting. |
| Redis dead-letter JSONL | implemented | P0 | Failed batches are appended to `data/redis_dead_letters.jsonl` after retries so events are inspectable instead of silently discarded. |
| Event type metrics | implemented | P0 | Router counts every `DataEvent` via `events_ingested_total{event_type=...}` and `bus_events_published_total{event_type=...}`; legacy tick counters remain. |
| Lock-free event snapshots | implemented | P0 | Latest-state caches use ArcSwap copy-on-write maps for lock-free readers and isolated writer swaps. |
| Async router snapshot publishing | implemented | P0 | Router hands bus/snapshot publication to a worker before forwarding original events to the aggregator. |
| Extended EventBus broadcast | implemented | P1 | `subscribe_events()` broadcasts raw `DataEvent`; high-volume domains also have isolated broadcast channels. |
| CEX websocket reconnect framework | partial | P1 | Shared `run_reconnecting` exists and is wired into Hyperliquid, Backpack, and dYdX; remaining legacy adapters should migrate incrementally. |
| Per-domain websocket subscriptions | implemented | P0 | `/v1/stream` can subscribe to quote/funding/OI/trade/liquidation/book/external_signal domains without unrelated quote receivers. |
| Large configurable broadcast buffers | implemented | P0 | `runtime.broadcast_capacity` defaults to 65,536; slow subscribers lag only their own receiver. |
| Slow websocket isolation | implemented | P0 | WS sends have a 3s timeout; slow clients are disconnected without blocking other subscribers. |
| Order-book level arbitrage | implemented | P1 | Spread engine emits `book_signal` from L2 depth using a conservative fixed 1,000 USDT notional. |
| Maker fee modeling | implemented | P2 | `strategy.fee_mode` supports `taker`, `maker`, `maker_buy_taker_sell`, and `taker_buy_maker_sell`. |
| Dynamic catalog from runtime config | implemented | P2 | `/v1/catalog/sources` reports `enabled`, `available`, or `enabled_missing_api_key` from the active config. |
| Aggregator extended event analytics | planned | P1 | Funding/OI/book/trade/liquidation are stored by API but ignored by spread engine. |
| Cross-platform release binaries | implemented | P1 | GitHub Actions builds v0.0.1 Linux/macOS/Windows packages with binary, configs, README, and docs. |
| SQLite kline store | implemented | P1 | `klines.enabled` stores Binance/OKX historical candles and realtime tick bars in `klines.sqlite_path`. |
