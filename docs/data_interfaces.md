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

Version `v0.0.2` is shipped as downloadable binary packages from GitHub Actions
and GitHub Releases.

| Platform | File |
|---|---|
| Linux 64-bit x86 | `market-bridge-v0.0.2-linux-x86_64.tar.gz` |
| Linux 32-bit x86 | `market-bridge-v0.0.2-linux-i686.tar.gz` |
| macOS Intel | `market-bridge-v0.0.2-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `market-bridge-v0.0.2-macos-aarch64.tar.gz` |
| Windows 64-bit | `market-bridge-v0.0.2-windows-x86_64.zip` |

Linux/macOS:

```bash
tar -xzf market-bridge-v0.0.2-linux-x86_64.tar.gz
cd market-bridge-v0.0.2-linux-x86_64
chmod +x ./market-bridge
MARKETBRIDGE_CONFIG=./config.yaml ./market-bridge
```

Windows PowerShell:

```powershell
Expand-Archive .\market-bridge-v0.0.2-windows-x86_64.zip
cd .\market-bridge-v0.0.2-windows-x86_64\market-bridge-v0.0.2-windows-x86_64
$env:MARKETBRIDGE_CONFIG = ".\config.yaml"
.\market-bridge.exe
```

Smoke checks:

```bash
curl -s http://127.0.0.1:8080/health
curl -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
curl -s "http://127.0.0.1:8080/v1/market/quotes?symbols=BTCUSDT" | jq
```

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
| Open interest | `/v1/market/open-interest` | CEX perp feeds | raw normalized | Latest OI rows. |
| Liquidations | `/v1/market/liquidations` | CEX feeds/REST | raw normalized | Venue support varies. |
| L2 books | `/v1/market/order-books` | CEX feeds | raw normalized | Latest depth snapshots. |
| Trades | `/v1/market/trades` | CEX feeds | raw normalized | Latest trade per venue/symbol cache. |
| Klines | `/v1/market/klines` | Binance/OKX REST + live ticks | stored + derived | SQLite OHLCV bars. |
| Basis | `/v1/market/basis` | quote snapshots | derived | Spot-perp basis per exchange/symbol. |
| Order flow | `/v1/market/order-flow` | trade events | derived | Buy/sell pressure buckets and CVD. |

## Klines

Config:

```yaml
klines:
  enabled: true
  sqlite_path: "data/marketbridge.sqlite"
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
| GET | `/v1/catalog/sources` | Source availability and API-key status. |
| GET | `/v1/catalog/source-roadmap` | External source expansion inventory with MarketBridge implementation status; reference-only, not a runtime dependency. |
| GET | `/v1/catalog/domains` | Normalized domain inventory. |
| GET | `/v1/catalog/instruments` | Instruments visible in live caches. |
| GET | `/v1/catalog/health` | Domain/source counts and freshness. |
| GET | `/v1/market/quotes` | Spot/perp/DeFi/TradFi/aggregate quote snapshots. |
| GET | `/v1/market/basis` | Spot-perp basis derived from quote snapshots. |
| GET | `/v1/market/funding` | Funding rates. |
| GET | `/v1/market/open-interest` | Open interest. |
| GET | `/v1/market/liquidations` | Liquidation events. |
| GET | `/v1/market/order-books` | L2 order books. |
| GET | `/v1/market/trades` | Recent trades. |
| GET | `/v1/market/order-flow` | Buy/sell pressure and CVD windows. |
| GET | `/v1/market/klines` | SQLite-backed OHLCV bars. |
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

## Recommended Research Order

1. Use `/v1/market/klines` for historical regime and backtest context.
2. Use `/v1/market/basis` for spot-perp dislocation.
3. Use `/v1/market/order-flow` for short-horizon buy/sell pressure.
4. Use `/v1/onchain/transfers` as a whale-event feature.
5. Let strategy systems such as PolyAlpha join these features with Polymarket
   prices and perform paper validation.
