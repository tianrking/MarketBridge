# MarketBridge Data Interfaces

This document is the practical interface map for consumers such as PolyAlpha.
It lists what data exists, whether it is raw or derived, required config, and
how to query it.

For the full source-by-source key matrix, use
[`data_sources.md`](data_sources.md). This file is endpoint-oriented; that file
is source/operator-oriented.

## Service Role

MarketBridge is the public data plane:

- collect public exchange, options, prediction-market, DeFi, macro, sentiment,
  and on-chain data;
- normalize data into stable REST/WebSocket surfaces;
- expose freshness and source health;
- compute reusable market microstructure features that are pure data transforms.

MarketBridge does not approve factors, run paper/live PnL, sign wallets, or
place orders.

## Release Binary Quick Start

Version `v0.0.5` is shipped as downloadable binary packages from GitHub Actions
and GitHub Releases.

Download the latest release from:
[https://github.com/tianrking/MarketBridge/releases/latest](https://github.com/tianrking/MarketBridge/releases/latest)

All releases are listed at:
[https://github.com/tianrking/MarketBridge/releases](https://github.com/tianrking/MarketBridge/releases)

| Platform | File |
|---|---|
| Linux 64-bit x86 | `market-bridge-v0.0.5-linux-x86_64.tar.gz` |
| Linux 32-bit x86 | `market-bridge-v0.0.5-linux-i686.tar.gz` |
| macOS Intel | `market-bridge-v0.0.5-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `market-bridge-v0.0.5-macos-aarch64.tar.gz` |
| Windows 64-bit | `market-bridge-v0.0.5-windows-x86_64.zip` |

Linux/macOS:

```bash
tar -xzf market-bridge-v0.0.5-linux-x86_64.tar.gz
cd market-bridge-v0.0.5-linux-x86_64
chmod +x ./market-bridge
MARKETBRIDGE_CONFIG=./config.yaml ./market-bridge
```

Windows PowerShell:

```powershell
Expand-Archive .\market-bridge-v0.0.5-windows-x86_64.zip
cd .\market-bridge-v0.0.5-windows-x86_64\market-bridge-v0.0.5-windows-x86_64
$env:MARKETBRIDGE_CONFIG = ".\config.yaml"
.\market-bridge.exe
```

Smoke checks:

```bash
curl -s http://127.0.0.1:8080/health
curl -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
curl -s "http://127.0.0.1:8080/v1/market/quotes?symbols=BTCUSDT" | jq
```

If `runtime.api_key` is configured, or the environment variable named by
`runtime.api_key_env` is present, send either header:

```bash
curl -H "x-api-key: $MARKETBRIDGE_API_KEY" \
  -s "http://127.0.0.1:8080/v1/market/quotes?symbols=BTCUSDT" | jq

curl -H "authorization: Bearer $MARKETBRIDGE_API_KEY" \
  -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
```

Set `runtime.api_rate_limit_per_minute` to a positive value to enable the
in-process per-client limiter. `0` disables rate limiting for local research.

## Hosted UI To Local Service

MarketBridge is designed to support a hosted static UI that calls a local
service running on the user's machine:

```text
Cloudflare Pages / Vercel static UI -> browser -> http://127.0.0.1:8080
```

The backend exposes browser-friendly CORS and Private Network Access preflight
handling through `runtime.cors`. Defaults allow local development origins,
Cloudflare Pages previews, and Vercel previews:

```yaml
runtime:
  cors:
    enabled: true
    allowed_origins:
      - "http://localhost:*"
      - "http://127.0.0.1:*"
      - "https://*.pages.dev"
      - "https://*.vercel.app"
    allow_private_network: true
    max_age_secs: 600
```

Browser preflight `OPTIONS` requests are allowed through API-key protection.
Real data requests still require `x-api-key` or `authorization: Bearer <key>`
when API auth is configured.

UI clients should probe:

```bash
curl -s "http://127.0.0.1:8080/v1/system/info" | jq
curl -s "http://127.0.0.1:8080/health" | jq
```

Synthetic local load test:

```bash
./market-bridge load-test --events 100000 --subscribers 8 --broadcast-capacity 65536 --event-bus-shards 1
```

This mode does not connect to exchanges. It publishes synthetic normalized
events through the same in-process bus and prints JSON throughput metrics.
Raise `--event-bus-shards` when measuring sharded event/domain broadcast
performance.

Use `config.min.yaml` for a small smoke test, `config.yaml` for normal local
research, and `config.all-exchanges.example.yaml` as an editable broad-coverage
example.

## Source And API-Key Matrix

The complete matrix is maintained in [`data_sources.md`](data_sources.md).
Short version:

| Family | Key requirement |
|---|---|
| CEX/perp public feeds | Mostly keyless; Architect uses `ARCHITECT_API_TOKEN`, Decibel uses `DECIBEL_API_TOKEN`. |
| Options | Keyless for Deribit/OKX/Bybit/Binance public data. |
| Polymarket data | Keyless for current Gamma/CLOB data paths. |
| DeFi | Mostly keyless; custom gateways may need config-specific keys outside the default setup. |
| Macro/aggregate/sentiment | Mixed: CoinGecko/CoinCap optional, CoinMarketCap/CoinGlass/FRED/CryptoPanic/Santiment/LunarCrush required. |
| On-chain transfers | Whale Alert and Etherscan require keys; mempool.space is keyless. |

## Core Market Data

| Data | Endpoint | Source | Type | Notes |
|---|---|---|---|---|
| Quotes | `/v1/market/quotes` | CEX/DeFi/TradFi/aggregates | raw normalized | Current latest quote snapshots. |
| Funding | `/v1/market/funding` | CEX perp feeds | raw normalized | Latest funding-rate rows. |
| Perpetual funding | `/v1/market/perpetual-funding` | CEX public REST tickers/contracts | raw normalized on demand | Pulls current funding rows for supported perp markets; not limited to configured symbols. |
| Open interest | `/v1/market/open-interest` | CEX perp feeds | raw normalized | Latest OI rows. |
| Liquidations | `/v1/market/liquidations` | CEX feeds/REST | raw normalized | Venue support varies. |
| L2 books | `/v1/market/order-books` | CEX feeds | raw normalized | Latest depth snapshots. |
| Trades | `/v1/market/trades` | CEX feeds | raw normalized | Latest trade per venue/symbol cache. |
| Klines | `/v1/market/klines` | Binance/OKX REST + live ticks | stored + derived | SQLite OHLCV bars; optional `persist=true` writes requested rows to local Parquet lake. |
| History candles | `/v1/history/candles` | Binance/OKX public history | raw normalized | On-demand `spot`, `futures/perp`, `mark`, `index`, `premiumIndex` where available, and `funding_rate` candles. |
| Basis | `/v1/market/basis` | quote snapshots | derived | Spot-perp basis per exchange/symbol. |
| Order flow | `/v1/market/order-flow` | trade events | derived | Buy/sell pressure buckets and CVD. |
| Order-flow windows | `/v1/market/order-flow/windows` | trade events | derived | Multi-window order-flow and CVD query. |
| Footprint | `/v1/market/footprint` | trade events | derived | Price-bin footprint, imbalance, stacked imbalance, and optional raw trades. |
| Universe filters | `/v1/universe/*` | klines, quotes, external signals | derived | Volume, percent change, volatility, spread, cross-market, market cap, age/listing, and delist-risk filters. |
| Research features | `/v1/research/features` | klines, quotes, funding, OI, books | derived | Multi-timeframe features, correlated assets, basis/funding/OI/liquidity regimes. |
| Strategy state | `/v1/research/symbol-state` | live events | derived | Real-time short-squeeze and exhaustion-short state machines with CVD, OFI, OI change, depth pressure, liquidation windows, and read-only risk context. |
| Agent context | `/v1/agent/context` | live snapshots + manifest | derived | AI-friendly read-only context bundle. |
| Local lake manifest | `/v1/storage/manifest` | SQLite manifest | metadata | Local Parquet lake index and data-quality metadata. |

## ClickHouse Tick Store

MarketBridge can optionally persist high-volume live events to ClickHouse over
the HTTP interface. This is intended for order-book/trade/OI replay,
millisecond-scale research queries, and feature recalculation outside the
in-memory latest-state cache.

Minimal local ClickHouse:

```bash
docker run --rm -p 8123:8123 -p 9000:9000 --name marketbridge-clickhouse clickhouse/clickhouse-server:latest
```

Enable the sink:

```yaml
runtime:
  clickhouse:
    enabled: true
    url: "http://127.0.0.1:8123"
    database: "marketbridge"
    password_env: CLICKHOUSE_PASSWORD
    batch_max: 1000
    flush_ms: 250
    local_buffer: 100000
    init_tables: true
```

Created tables:

| Table | Stored events |
|---|---|
| `marketbridge.market_quotes` | spot/perp quote ticks and mark/funding fields when present |
| `marketbridge.trades` | trade ticks with side, price, quantity, and trade id |
| `marketbridge.order_books` | latest emitted L2 books with best bid/ask plus bid/ask JSON levels |
| `marketbridge.funding_rates` | funding-rate ticks |
| `marketbridge.open_interest` | OI ticks |
| `marketbridge.liquidations` | liquidation ticks |
| `marketbridge.external_signals` | CoinGlass, sentiment, DeFi native-state, and custom API signals |

Example queries:

```sql
SELECT
  symbol,
  exchange,
  count() AS trades,
  sum(if(side = 'buy', price * qty, -price * qty)) AS cvd_notional
FROM marketbridge.trades
WHERE symbol = 'BTCUSDT'
  AND ts_ms >= toUnixTimestamp64Milli(now64(3) - INTERVAL 5 MINUTE)
GROUP BY symbol, exchange;

SELECT
  symbol,
  exchange,
  argMax(open_interest, ts_ms) AS latest_oi
FROM marketbridge.open_interest
WHERE symbol = 'BTCUSDT'
GROUP BY symbol, exchange;
```

## Klines

Config:

```yaml
klines:
  enabled: true
  sqlite_path: "data/marketbridge.sqlite"
  lake_root: "data/lake"
  intervals: [1m, 5m, 15m, 1h]
  history_limit: 1500
  backfill_on_start: false
  sources: [binance, okx]
```

Behavior:

- Historical REST backfill supports Binance spot/perp and OKX spot/swap.
- Realtime candles are aggregated from live quote ticks.
- SQLite stores one row per `exchange + market + symbol + interval + open_time_ms`.
- `backfill_on_start: false` avoids unexpected exchange REST bursts. Turn it on
  when intentionally seeding history.

Query:

```bash
curl -s "http://127.0.0.1:8080/v1/market/klines?exchange=binance&market=perp&symbol=BTCUSDT&interval=1m&limit=100" | jq
```

Persist only selected rows to the local Parquet lake:

```bash
curl -s "http://127.0.0.1:8080/v1/market/klines?exchange=binance&market=perp&symbol=BTCUSDT&interval=1m&limit=1000&persist=true" | jq
```

## Historical Candle Lake

`/v1/history/candles` fetches specific public candle types on demand. It does
not write to the lake unless `persist=true` is set.

Supported public history:

- Binance: `spot`, `futures`, `perp`, `mark`, `index`, `premiumIndex`,
  `funding_rate`
- OKX: `spot`, `perp`, `mark`, `index`, `funding_rate`

Examples:

```bash
curl -s "http://127.0.0.1:8080/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=mark&interval=1m&limit=1000&persist=true" | jq
curl -s "http://127.0.0.1:8080/v1/history/candles?exchange=okx&symbol=BTCUSDT&candle_type=funding_rate&limit=100&persist=true" | jq
```

Manifest and deletion:

```bash
curl -s "http://127.0.0.1:8080/v1/storage/manifest?domain=candles&symbol=BTCUSDT" | jq
curl -X DELETE "http://127.0.0.1:8080/v1/storage/partitions?domain=candles&symbol=BTCUSDT&candle_type=mark" | jq
```

Params:

- `exchange`
- `market=spot|perp`
- `symbol`
- `interval=1m|3m|5m|15m|30m|1h|4h|1d`
- `start_ms`, `end_ms`
- `limit`, default `500`, max `5000`

## Basis

Basis is a derived metric:

```text
spot_mid = (spot_bid + spot_ask) / 2
perp_mid = (perp_bid + perp_ask) / 2
basis = perp_mid - spot_mid
basis_bps = basis / spot_mid * 10000
```

Query:

```bash
curl -s "http://127.0.0.1:8080/v1/market/basis?symbols=BTCUSDT&exchanges=binance,okx" | jq
```

Notes:

- No new data source is required.
- A row appears only when the same exchange has both fresh spot and perp quotes
  for the symbol.

## Order Flow

Order flow is derived from live trade events.

Windows:

- `60000` ms
- `300000` ms
- `900000` ms

Query:

```bash
curl -s "http://127.0.0.1:8080/v1/market/order-flow?exchange=binance&market=perp&symbol=BTCUSDT&window_ms=60000" | jq
```

Fields:

- `buy_qty`, `sell_qty`
- `buy_notional`, `sell_notional`
- `delta_qty`, `delta_notional`
- `cumulative_delta_qty`, `cumulative_delta_notional`
- `trade_count`, `large_trade_count`

Notes:

- Order flow exists only for venues with implemented trade feeds.
- The current large-trade threshold is 100,000 USDT notional.

## On-chain Transfers

| Source | Chain | Key | Scope |
|---|---|---|---|
| Whale Alert | multi-chain | `WHALE_ALERT_API_KEY` | Global large-transfer API. |
| mempool.space | Bitcoin | none | Recent BTC mempool poller. |
| Etherscan | Ethereum | `ETHERSCAN_API_KEY` | Configured address watchlist. |

Config:

```yaml
onchain:
  whale_alert:
    enabled: true
    api_key_env: WHALE_ALERT_API_KEY
    min_value_usd: 500000
  mempool_space:
    enabled: true
    min_value_btc: 100
  etherscan:
    enabled: true
    api_key_env: ETHERSCAN_API_KEY
    min_value_eth: 1000
    safe_confirmations: 12
    request_delay_ms: 250
    addresses:
      - "0x..."
```

Query:

```bash
curl -s "http://127.0.0.1:8080/v1/onchain/transfers?source=whale_alert&min_amount_usd=1000000" | jq
curl -s "http://127.0.0.1:8080/v1/onchain/transfers?source=mempool_space&chain=bitcoin&asset=BTC" | jq
curl -s "http://127.0.0.1:8080/v1/onchain/transfers?source=etherscan&chain=ethereum&asset=ETH" | jq
```

Important boundaries:

- Whale Alert is the simplest global source but requires a key.
- mempool.space is keyless and BTC-only here. It is useful as an early warning
  feed, not a full labeled whale classifier.
- Etherscan is address-watchlist based in this project. Full-chain Ethereum
  transfer firehose requires an archive/indexing provider or node stack.
- Etherscan polling waits for `safe_confirmations` and queries only up to that
  safe block. Requests are also spaced by `request_delay_ms` and retried with
  backoff on rate-limit or transient server responses.

## Prediction, Options, External Data

| Data | Endpoint | Notes |
|---|---|---|
| Options chains | `/v1/options/chains` | Deribit/OKX/Bybit/Binance REST cache. |
| Option books | `/options/deribit/book`, `/options/okx/book`, `/options/bybit/book`, `/options/binance/book` | Keyless per-instrument option depth. |
| Polymarket Gamma discovery | `/polymarket/markets`, `/polymarket/crypto-markets` | General active Gamma markets plus the BTC/ETH crypto parser. |
| Polymarket books | `/v1/prediction/books` | Live CLOB cache. |
| Polymarket batch prices | `/polymarket/midpoints`, `/polymarket/spreads`, `/polymarket/prices`, `/polymarket/last-trade-prices` | Public CLOB wrappers. |
| Polymarket price history | `/polymarket/prices-history` | Public CLOB history/OHLCV wrapper. |
| External signals | `/v1/external/signals` | CoinGlass, Fear & Greed, CryptoPanic, Santiment, LunarCrush, and DeFi native-state metrics emitted by pool connectors. |

Known non-Polymarket gaps are centralized in
[`feature_inventory.md`](feature_inventory.md#remaining-non-polymarket-data-gaps).
Do not infer missing domains from empty API responses: many venues simply do not
publish stable public liquidation, OI, or trade semantics for every product.

## Streaming

Live domain stream:

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=market_quote,trade,order_book&symbols=BTCUSDT&product_type=perp"
```

Supported live domains include:

- `market_quote`
- `funding`
- `open_interest`
- `trade`
- `liquidation`
- `order_book`
- `external_signal`
- `options_chain` and `prediction_book` as snapshot streams

## REST and WebSocket Surface

Base URL: `http://127.0.0.1:8080`

| Method | Path | Data |
|---|---|---|
| GET | `/` | Service metadata. |
| GET | `/health` | Service liveness. |
| GET | `/v1/system/info` | Version, API version, local UI connection hints, and capability list. |
| GET | `/v1/catalog/sources` | Source availability and API-key status. |
| GET | `/v1/catalog/search` | Product search: where an asset/symbol trades and which data domains are available. |
| GET | `/v1/catalog/markets` | On-demand platform market/symbol discovery. |
| GET | `/v1/catalog/perpetuals` | Grouped on-demand perpetual contract discovery by exchange. |
| GET | `/v1/catalog/source-roadmap` | External source expansion inventory with MarketBridge implementation status; reference-only, not a runtime dependency. |
| GET | `/v1/catalog/domains` | Normalized domain inventory. |
| GET | `/v1/catalog/instruments` | Instruments visible in live caches. |
| GET | `/v1/catalog/health` | Domain/source counts and freshness. |
| GET | `/v1/market/quotes` | Spot/perp/DeFi/TradFi/aggregate quote snapshots. |
| GET | `/v1/market/basis` | Spot-perp basis derived from quote snapshots. |
| GET | `/v1/market/funding` | Funding rates. |
| GET | `/v1/market/perpetual-funding` | On-demand current funding rows for perpetual markets. |
| GET | `/v1/market/open-interest` | Open interest. |
| GET | `/v1/market/liquidations` | Liquidation events. |
| GET | `/v1/market/order-books` | L2 order books. |
| GET | `/v1/market/trades` | Recent trades. |
| GET | `/v1/market/order-flow` | Buy/sell pressure and CVD windows. |
| GET | `/v1/market/order-flow/windows` | Multi-window order-flow and CVD. |
| GET | `/v1/market/footprint` | Footprint/orderflow profile. |
| GET | `/v1/market/klines` | SQLite-backed OHLCV bars with optional Parquet persistence. |
| GET | `/v1/history/candles` | On-demand special candle history. |
| GET | `/v1/storage/manifest` | Local Parquet lake manifest and quality metadata. |
| DELETE | `/v1/storage/partitions` | Delete local lake partitions by filter. |
| GET | `/v1/universe/top-volume` | Universe by volume. |
| GET | `/v1/universe/percent-change` | Universe by percent change. |
| GET | `/v1/universe/volatility` | Universe by realized volatility. |
| GET | `/v1/universe/spread-filter` | Universe by current spread. |
| GET | `/v1/universe/cross-market` | Cross-market availability. |
| GET | `/v1/universe/market-cap` | Market-cap ranking. |
| GET | `/v1/universe/age-filter` | Listing-age filter. |
| GET | `/v1/universe/new-listings` | Recent listing candidates. |
| GET | `/v1/universe/delist-risk` | Stale/missing quote risk for historical markets. |
| GET | `/v1/research/features` | Research feature package. |
| GET | `/v1/research/market-regime` | Aggregate market regime snapshot. |
| GET | `/v1/research/symbol-state` | Real-time per-symbol squeeze/exhaustion state machine. |
| GET | `/v1/agent/context` | Agent-friendly read-only context. |
| GET | `/v1/agent/capabilities` | Agent-friendly capability inventory. |
| GET | `/v1/options/chains` | Cached option chains. |
| GET | `/v1/prediction/books` | Cached Polymarket books. |
| GET | `/v1/external/signals` | Aggregates, macro, news, and sentiment. |
| GET | `/v1/onchain/transfers` | Large transfer feed. |
| GET | `/snapshot` | Legacy latest quote tick snapshot. |
| GET | `/funding` | Legacy unified funding view. |
| GET | `/options/deribit/summary` | Live Deribit REST option summary. |
| GET | `/options/deribit/live-summary` | Cached Deribit option summary with freshness fields. |
| GET | `/options/deribit/book` | Deribit per-instrument option book plus greeks where returned. |
| GET | `/options/okx/book` | OKX per-instrument option book. |
| GET | `/options/bybit/book` | Bybit per-instrument option book. |
| GET | `/options/binance/book` | Binance per-instrument option book. |
| GET | `/polymarket/markets` | General active Polymarket Gamma markets with CLOB ids and outcomes. |
| GET | `/polymarket/crypto-markets` | Parsed BTC/ETH Polymarket crypto markets. |
| GET | `/polymarket/book` | Single Polymarket token order book. |
| GET | `/polymarket/books` | Batch Polymarket token order books. |
| GET | `/polymarket/midpoints` | Batch public midpoint prices. |
| GET | `/polymarket/spreads` | Batch public spreads. |
| GET | `/polymarket/last-trade-prices` | Batch public last-trade prices. |
| GET | `/polymarket/prices` | Batch public BUY/SELL executable prices. |
| GET | `/polymarket/prices-history` | Single or batch public price history. |
| GET | `/polymarket/crypto-books` | Crypto markets plus REST books. |
| GET | `/polymarket/live-books` | Cached Polymarket books seeded by REST and patched by websocket. |
| GET | `/polymarket/live-crypto-books` | Crypto markets plus cached books. |
| GET | `/coverage` | Data quality dashboard model. |
| GET | `/metrics` | Prometheus metrics. |
| WS | `/v1/stream` | Domain-filtered live stream. |
| WS | `/ws/ticks` | Legacy quote tick stream. |

## Market Discovery And Raw Perpetual Data

Use `/v1/catalog/search` when a client has a user-facing product input and needs
one answer for "where does this trade and what data can MarketBridge provide?".
The endpoint accepts `q`, `product`, `base`, or `symbol`, then returns normalized
listings plus data domains, derived metrics, and ready-to-call REST/WebSocket
paths for each market.

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/search?q=HOME" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/search?q=HOMEUSDT&market=perp" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/search?base=HOME&exchanges=binance,okx,bybit,bitget,gate,mexc" | jq
```

Use `/v1/catalog/markets` when you need the latest public market list from a
venue instead of only the symbols currently configured for live ingestion.

Query params:

| Param | Required | Description |
|---|---:|---|
| `exchange` | no | Single exchange id, for example `binance`. Use exactly one of `exchange` or `exchanges` for targeted discovery. |
| `exchanges` | no | Comma-separated exchange ids, for example `binance,okx,bybit`. |
| `market` | no | `spot`, `perp`, or `swap`; omit to include all supported market types for the venue. |
| `quote` | no | Quote filter such as `USDT`. |
| `base` | no | Base asset filter such as `BTC`. |
| `active_only` | no | `true` by default. Set `false` to include inactive/non-trading listings when the venue exposes them. |
| `limit` | no | Default `5000`, max `50000`. |

Response fields:

| Field | Meaning |
|---|---|
| `version` | API version, currently `v1`. |
| `domain` | `catalog_markets`. |
| `supported_exchanges` | Exchanges implemented by this on-demand adapter layer. |
| `markets[]` | Normalized listing rows. |
| `errors[]` | Per-exchange request or parsing failures. Non-empty errors mean the client should treat the response as partial. |

`markets[]` rows include:

| Field | Meaning |
|---|---|
| `exchange` | Normalized exchange id. |
| `market` | `spot` or `perp`. |
| `symbol` | Normalized symbol, usually `BASEQUOTE` such as `BTCUSDT`. |
| `native_symbol` | Venue-native symbol/id. |
| `base`, `quote` | Parsed assets when available. |
| `active` | Whether the venue reports the market as active/trading. |
| `status` | Venue status string when available. |
| `contract_type` | Contract type such as `PERPETUAL` when available. |
| `settle_asset` | Settlement or margin asset when available. |
| `source` | Public REST URL used by the adapter. |

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/markets?exchange=binance&market=perp&quote=USDT&limit=20" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/markets?exchanges=okx,bybit,bitget&market=spot&quote=USDT" | jq
```

Use `/v1/catalog/perpetuals` for the direct "which perpetual contracts does this
exchange list?" question. Each exchange adapter handles its own native REST
format; the response is grouped by exchange and includes normalized symbols,
native symbols, and unique base assets.

Query params:

| Param | Required | Description |
|---|---:|---|
| `exchange` | no | Single exchange id. |
| `exchanges` | no | Comma-separated exchange ids. |
| `quote` | no | Quote filter such as `USDT`. |
| `base` | no | Base filter such as `BTC`. |
| `active_only` | no | `true` by default. |
| `limit` | no | Per-exchange returned contract cap, default `50000`. |

Response fields:

| Field | Meaning |
|---|---|
| `version` | API version, currently `v1`. |
| `domain` | `catalog_perpetuals`. |
| `supported_exchanges` | Exchanges implemented by this on-demand adapter layer. |
| `exchanges[]` | One grouped result per exchange. |
| `errors[]` | Per-exchange failures; non-empty errors indicate partial data. |

`exchanges[]` rows include:

| Field | Meaning |
|---|---|
| `exchange` | Normalized exchange id. |
| `contracts_total` | Total matching contracts before the per-exchange `limit` is applied. |
| `contracts_returned` | Number of rows returned in `contracts`. |
| `base_assets_total` | Unique base asset count. |
| `base_assets` | Sorted unique base assets. |
| `contracts` | Listing rows with the same fields as `/v1/catalog/markets`. |

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/perpetuals?exchange=okx&quote=USDT" | jq
curl -s "http://127.0.0.1:8080/v1/catalog/perpetuals?exchanges=binance,bybit,bitget&quote=USDT" | jq
```

Use `/v1/market/perpetual-funding` to retrieve current funding rows for those
perpetual markets. MarketBridge returns the raw normalized data; clients should
apply their own thresholds, watchlists, alerting, and monitoring logic.

Query params:

| Param | Required | Description |
|---|---:|---|
| `exchange` | no | Single exchange id. |
| `exchanges` | no | Comma-separated exchange ids. |
| `symbols` | no | Comma-separated normalized symbols, e.g. `BTCUSDT,ETHUSDT`. |
| `quote` | no | Quote filter such as `USDT`. |
| `active_only` | no | `true` by default. |
| `limit` | no | Default `5000`, max `50000`. |

Response fields:

| Field | Meaning |
|---|---|
| `version` | API version, currently `v1`. |
| `domain` | `market_perpetual_funding`. |
| `supported_exchanges` | Exchanges implemented by this on-demand funding adapter layer. |
| `funding[]` | Current funding rows. |
| `errors[]` | Per-exchange failures; non-empty errors indicate partial data. |

`funding[]` rows include:

| Field | Meaning |
|---|---|
| `exchange` | Normalized exchange id. |
| `symbol` | Normalized symbol, usually `BASEQUOTE`. |
| `native_symbol` | Venue-native symbol/id. |
| `funding_rate` | Decimal funding rate, e.g. `-0.001`. |
| `funding_rate_pct` | Percent funding rate, e.g. `-0.1` means `-0.1%`. |
| `next_funding_time_ms` | Next funding timestamp in Unix milliseconds when available. |
| `mark_price`, `index_price` | Venue mark/index price when available. |
| `active` | Whether the venue reports the contract as active/trading when available. |
| `source` | Public REST URL used by the adapter. |
| `ts_ms` | MarketBridge fetch/normalization timestamp in Unix milliseconds. |

```bash
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchange=bybit&quote=USDT&limit=5000" | jq
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchanges=binance,okx,bitget&symbols=BTCUSDT,ETHUSDT" | jq
```

For a client-side `curl + jq` filter, such as Binance contracts between `-2%`
and `-0.2%`, use:

```bash
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

For more copy-paste searches, CSV exports, cross-exchange comparisons, and
watchlist generation, see
[`perpetual_funding_cookbook.md`](perpetual_funding_cookbook.md).

Supported first-pass discovery/funding venues are Binance, OKX, Bybit, Bitget,
KuCoin, Gate, MEXC, BingX, and Bitmart. Existing live-cache endpoints such as
`/v1/market/funding` remain faster for configured symbols; the on-demand
perpetual endpoints are for broad data discovery and client-side selection.

## Recommended Research Order

For concrete short-squeeze and exhaustion-short examples, see
[`squeeze_and_exhaustion_examples.md`](squeeze_and_exhaustion_examples.md).

1. Use `/v1/market/klines` for historical regime and backtest context.
2. Use `/v1/market/basis` for spot-perp dislocation.
3. Use `/v1/market/order-flow` for short-horizon buy/sell pressure.
4. Use `/v1/onchain/transfers` as a whale-event feature.
5. Let strategy systems such as PolyAlpha join these features with Polymarket
   prices and perform paper validation.
