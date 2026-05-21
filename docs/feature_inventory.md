# MarketBridge Feature Inventory

This document is the operating checklist for MarketBridge coverage. Update it in
the same commit as every connector, API, or infrastructure change.

Broader external source coverage candidates are tracked in
[`source_expansion_inventory.md`](source_expansion_inventory.md). That file uses
CCXT and Hummingbot only as reference inventories. MarketBridge does not call,
embed, bridge, or depend on them at runtime. This document remains the source of
truth for what is implemented and wired into the runtime/API.

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
| Source roadmap | `/v1/catalog/source-roadmap` | implemented | keyless | External source expansion inventory; not runtime data coverage. |
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
| Hyperliquid | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp DEX source; `activeAssetCtx` emits funding and OI from the same update when both fields are present. |
| dYdX v4 | partial | implemented | implemented | implemented | implemented | planned | keyless | REST market metadata plus WS book/trades. |
| Backpack | partial | implemented | implemented | implemented | implemented | planned | keyless | Public WS book/trade plus perp mark-price funding and open-interest REST polling. |
| MEXC | partial | implemented | implemented | implemented | n/a | n/a | keyless | Spot/futures depth/deals plus public contract funding-rate poller; CCXT reference marks public open-interest and public liquidation fetches unavailable. |
| BingX | partial | implemented | implemented | implemented | implemented | n/a | keyless | Swap ticker/depth/trade plus public premium-index funding and open-interest poller; liquidation/force-order history is private-only in the CCXT reference. |
| Bitget | implemented | implemented | implemented | implemented | implemented | planned | keyless | Public WS ticker/books5/trade; perp ticker also emits funding and OI. |
| Bitmart | partial | implemented | implemented | implemented | implemented | n/a | keyless | Public spot depth/trades and perp depth/trades/funding/ticker plus public perp funding/OI REST poller; public liquidation fetch is unavailable in CCXT. |
| BitMEX | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Native public REST instrument/book/trades/liquidations for perpetual markets. |
| Bitstamp | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public spot order-book diffs and trades; derivatives domains do not apply to spot feed. |
| Bitrue | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public spot simple-depth stream plus recent-trade REST polling with trade-id dedupe; derivatives domains do not apply. |
| Bithumb | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| Bitvavo | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| BloFin | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST perp ticker, order book, trades, funding, and open interest. |
| bitFlyer | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| bitbank | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| AscendEX | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public spot order-book diffs and trades from Hummingbot WS endpoint; derivatives domains do not apply. |
| WOO X | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST spot/perp order books and trades plus perp funding and OI. |
| BTC Markets | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public spot order book and trades; derivatives domains do not apply. |
| Bullish | implemented | implemented | implemented | n/a | partial | planned | keyless | Native public REST spot ticker, order book, and recent trades; endpoint may be geofenced. |
| Aevo | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp order books/trades plus REST funding, instrument BBO, and instrument open interest. |
| Phemex | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST linear perp ticker/book/trades with funding and OI. |
| Pacifica | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp order books/trades/prices with funding and OI. |
| Upbit | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| GRVT | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp book/trade/ticker streams with funding and OI. |
| Vertex | partial | implemented | implemented | planned | planned | planned | keyless | Public CLOB spot/perp book-depth and trade streams for known product ids. |
| Injective | partial | implemented | implemented | implemented | implemented | planned | keyless | Public LCD/Sentry spot/perp order books and trades plus perp funding and OI pollers. |
| XRPL | partial | implemented | planned | planned | planned | planned | keyless | Public book_offers snapshots; issuer-aware mapping currently includes XRP/USD. |
| Architect | partial | implemented | implemented | implemented | planned | planned | keyed | Read-only perp WS book/trade and REST funding; requires bearer token. |
| Decibel | partial | implemented | implemented | implemented | planned | planned | keyed | Read-only Aptos Decibel perp depth/trades/market_price streams; requires bearer token and market address discovery. |
| Deribit | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST perp ticker, order book, trades, funding, and open interest. |
| Evedex | partial | implemented | implemented | implemented | implemented | planned | keyless | Public perp REST depth, recent trades, funding, and open-interest metrics. |
| Derive | partial | implemented | implemented | implemented | implemented | planned | keyless | Public spot/perp order books and trades plus perp ticker funding and open interest. |
| Dexalot | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public CLOB DEX spot order books and trades; derivatives domains do not apply. |
| KuCoin | implemented | implemented | implemented | implemented | implemented | n/a | keyless | Spot/perp ticker plus spot REST book/trades and futures REST book/trades/funding/OI; no stable public liquidation feed in the CCXT reference. |
| Gate | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Spot bookTicker plus REST book/trades; perp bookTicker plus REST book/trades/contracts funding/OI/liquidations. |
| Gemini | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and trades. |
| Kraken | implemented | implemented | implemented | implemented | implemented | n/a | keyless | Spot V2 ticker plus REST book/trades; futures REST ticker/book/trades/funding/OI with `PF_` symbol mapping. Public liquidation feed is not exposed in the CCXT reference. |
| HTX | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Spot BBO plus REST book/trades; linear swap BBO plus REST book/trades/funding/OI/liquidations. |
| Bitfinex | implemented | implemented | implemented | implemented | implemented | implemented | keyless | Spot/perp ticker; spot/perp REST book/trades, derivative status funding/OI, and public liquidation history filtered by symbol. |
| Coinbase | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Advanced Trade ticker plus Exchange REST order book and recent trades. |
| Coincheck | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| CoinEx | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST ticker/book/trades for spot/perp plus perp funding and OI. |
| Coinone | implemented | implemented | implemented | n/a | n/a | n/a | keyless | Native public REST spot ticker, order book, and recent trades. |
| Crypto.com | implemented | implemented | implemented | implemented | implemented | planned | keyless | Native public REST ticker/book/trades for spot/perp plus perp funding and OI. |
| Cube | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public REST MBP snapshots plus recent-trade polling with trade-id dedupe for configured spot markets; derivatives domains do not apply to the current spot connector. |
| Foxbit | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public Brazil spot REST order-book snapshots plus recent-trade REST polling with trade-id dedupe; derivatives domains do not apply. |
| NDAX | partial | implemented | implemented | n/a | n/a | n/a | keyless | Public Canadian spot L2 snapshots and GetLastTrades polling after GetInstruments symbol resolution; derivatives domains do not apply. |

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
| Gamma market discovery | implemented | keyless | `/polymarket/markets` exposes all active Gamma categories with CLOB ids, outcomes, volume, liquidity, and event OI; `/polymarket/crypto-markets` keeps the BTC/ETH parser. |
| REST book | implemented | keyless | `/polymarket/book`, `/polymarket/books`. |
| Live CLOB cache | implemented | keyless | `/polymarket/live-books`, `/v1/prediction/books`. |
| Midpoint batch | implemented | keyless | `/polymarket/midpoints` wraps CLOB `POST /midpoints`, max 500 token ids. |
| Last trade price batch | implemented | keyless | `/polymarket/last-trade-prices` wraps CLOB `POST /last-trades-prices`, max 500 token ids. |
| Spread batch | implemented | keyless | `/polymarket/spreads` wraps CLOB `POST /spreads`, max 500 token ids. |
| Market prices batch | implemented | keyless | `/polymarket/prices` wraps CLOB `POST /prices`, default sides `BUY,SELL`. |
| Historical prices/OHLCV | implemented | keyless | `/polymarket/prices-history` wraps CLOB `GET /prices-history` and `POST /batch-prices-history`, batch max 20 token ids. |
| Full category coverage | implemented | keyless | Active Gamma market discovery is category-agnostic; specialized parsers can layer on top. |
| Open interest/live volume | implemented | keyless | Gamma volume/liquidity/event OI fields are exposed when present. |

## Options Coverage

| Venue | Summary chain | Greeks | WS ticker/book/trades | Depth | API key | Notes |
|---|---:|---:|---:|---:|---:|---|
| Deribit | implemented | implemented | partial | implemented | keyless | Summary chain plus `/options/deribit/book` for per-instrument depth and delta/gamma/theta/vega. |
| OKX Options | implemented | partial | planned | planned | keyless | REST option summary. |
| Bybit Options | implemented | implemented | implemented | planned | keyless | Public ticker emits IV, OI, and delta/gamma/theta/vega. |
| Binance Options | implemented | partial | implemented | planned | keyless | Public ticker plus mark endpoint emits IV and delta/gamma/theta/vega. |

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
| Redis dead-letter JSONL | implemented | P0 | Failed batches are appended to `runtime.redis_dead_letter_path` after retries so events are inspectable instead of silently discarded. |
| Event type metrics | implemented | P0 | Router counts every `DataEvent` via `events_ingested_total{event_type=...}` and `bus_events_published_total{event_type=...}`; legacy tick counters remain. |
| Non-finite number guard | implemented | P0 | `SourceContext::emit` drops NaN/Inf events before they reach JSON, Redis, API caches, or TTL logic. |
| Lock-free event snapshots | implemented | P0 | Latest-state caches use ArcSwap copy-on-write maps for lock-free readers and isolated writer swaps. |
| Async router snapshot publishing | implemented | P0 | Router hands bus/snapshot publication to a worker before forwarding original events to the aggregator. |
| Extended EventBus broadcast | implemented | P1 | `subscribe_events()` broadcasts shared `Arc<DataEvent>` values; high-volume domains also have isolated broadcast channels. |
| CEX websocket reconnect framework | partial | P1 | Shared `run_reconnecting` exists and is wired into Hyperliquid, Backpack, and dYdX; remaining legacy adapters should migrate incrementally. |
| Per-domain websocket subscriptions | implemented | P0 | `/v1/stream` can subscribe to quote/funding/OI/trade/liquidation/book/external_signal domains without unrelated quote receivers. |
| Large configurable broadcast buffers | implemented | P0 | `runtime.broadcast_capacity` defaults to 65,536; slow subscribers lag only their own receiver. |
| Slow websocket isolation | implemented | P0 | WS sends use `runtime.ws_send_timeout_ms`; slow clients are disconnected without blocking other subscribers. |
| Websocket send error context | implemented | P0 | Stream send failures preserve timeout/socket/serialization context in logs. |
| Order-book level arbitrage | implemented | P1 | Spread engine emits `book_signal` from L2 depth using `strategy.book_signal_notional_usdt`. |
| Maker fee modeling | implemented | P2 | `strategy.fee_mode` supports `taker`, `maker`, `maker_buy_taker_sell`, and `taker_buy_maker_sell`. |
| Conservative fallback fees | implemented | P0 | Missing venue fee config uses `strategy.fallback_maker_fee_bps` / `strategy.fallback_taker_fee_bps` instead of zero-fee assumptions. |
| Dynamic catalog from runtime config | implemented | P2 | `/v1/catalog/sources` reports startup-cached `enabled`, `available`, or `enabled_missing_api_key` from the active config. |
| Aggregator extended event analytics | planned | P1 | Funding/OI/book/trade/liquidation are stored by API but ignored by spread engine. |
| Cross-platform release binaries | implemented | P1 | GitHub Actions builds v0.0.2 Linux/macOS/Windows packages with binary, configs, README, and docs. |
| SQLite kline store | implemented | P1 | `klines.enabled` stores Binance/OKX historical candles and realtime tick bars in `klines.sqlite_path`. |
