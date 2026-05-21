# MarketBridge Data Sources

This is the operator-facing source inventory. It answers three questions:

1. What data can MarketBridge collect?
2. Does the source need an API key?
3. Which endpoint should a downstream project call?

MarketBridge is data-only. It never places orders, signs requests, or fabricates
missing venue data. A `keyless` row means the public data path works without a
user credential. A `keyed` row means the connector can run only after the user
sets the required config value or environment variable. An `optional` key means
the public source may work without a key, but a key can raise limits or improve
reliability.

## Quick Start

Run with a config file:

```bash
MARKETBRIDGE_CONFIG=./config.yaml ./market-bridge
```

Check which sources are enabled and whether any required key is missing:

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/sources" | jq
```

Source status meanings:

| Status | Meaning |
|---|---|
| `enabled` | Enabled and usable from the active config. |
| `available` | Connector exists but is disabled in config. |
| `enabled_missing_api_key` | Enabled, but a required API key/token is missing. |

## API Key Setup

Prefer environment variables for credentials:

```bash
export COINGLASS_API_KEY="..."
export COINMARKETCAP_API_KEY="..."
export FRED_API_KEY="..."
export CRYPTOPANIC_API_KEY="..."
export SANTIMENT_API_KEY="..."
export LUNARCRUSH_API_KEY="..."
export WHALE_ALERT_API_KEY="..."
export ETHERSCAN_API_KEY="..."
export ARCHITECT_API_TOKEN="..."
export DECIBEL_API_TOKEN="..."
```

Most key-capable config blocks also support inline `api_key`, but environment
variables are safer for local development and CI.

## Core CEX And Perp Sources

Primary endpoints:

| Data | REST | WebSocket domain | Key |
|---|---|---|---|
| Spot/perp quotes | `/v1/market/quotes` | `market_quote` | Mixed by source, mostly keyless |
| L2 order books | `/v1/market/order-books` | `order_book` | Mixed by source, mostly keyless |
| Trades | `/v1/market/trades` | `trade` | Mixed by source, mostly keyless |
| Funding rates | `/v1/market/funding` | `funding` | Mixed by source, mostly keyless |
| Open interest | `/v1/market/open-interest` | `open_interest` | Mixed by source, mostly keyless |
| Liquidations | `/v1/market/liquidations` | `liquidation` | Keyless where public feeds exist |

Implemented keyless CEX/perp sources include:

| Source group | Data notes | Key |
|---|---|---|
| Binance / Bybit / OKX | BBO, L2, trades, funding, OI, liquidations where public. | keyless |
| Hyperliquid / dYdX / Backpack / MEXC / BingX / Bitget / Bitmart | Public perp/spot feeds; liquidation support only where stable public data exists. | keyless |
| BitMEX / Deribit / Phemex / CoinEx / Crypto.com / WOO X / BloFin / Aevo / Pacifica / GRVT / Injective / Derive / Evedex | Public derivatives data paths: books/trades/funding/OI by venue capability. | keyless |
| Coinbase / Kraken / KuCoin / Gemini / Bithumb / Bitvavo / bitFlyer / bitbank / Coincheck / Coinone / Upbit / Bullish | Public spot or spot+futures market data by venue. | keyless |
| Gate / HTX / Bitfinex / Bitstamp / Bitrue / AscendEX / BTC Markets / Dexalot / Vertex / XRPL / Cube / Foxbit / NDAX | Long-tail CEX/CLOB/DEX data where public contracts are stable. | keyless |

Keyed CEX/perp sources:

| Source | Data | Env var | Notes |
|---|---|---|---|
| Architect | Read-only perp book/trade/funding; OI normalized if keyed payload exposes it. | `ARCHITECT_API_TOKEN` | Bearer token required. |
| Decibel | Read-only Aptos perp depth/trades/market_price; OI normalized if payload exposes it. | `DECIBEL_API_TOKEN` | Bearer token and market-address discovery required. |

Important boundaries:

- `n/a` liquidation/OI/funding cells in `feature_inventory.md` mean the venue
  does not expose a stable public feed for that product.
- MarketBridge leaves those domains empty instead of synthesizing fake data.
- XRPL trades are parsed from validated transaction Offer fills, not inferred
  from `book_offers`.

## Derived Market Features

These do not need new external sources; they are computed from normalized market
events already collected by MarketBridge.

| Feature | Endpoint | Source data | Key |
|---|---|---|---|
| Spot-perp basis | `/v1/market/basis` | Latest spot/perp quote snapshots | keyless |
| Order flow | `/v1/market/order-flow` | Live trade events | keyless |
| Klines | `/v1/market/klines` | Historical REST candles plus live tick aggregation | keyless |
| Funding/OI/trade/liquidation analytics | Logs from `SpreadAggregator` | Normalized funding, OI, trade, liquidation, and book events | keyless |

Example:

```bash
curl -s "http://127.0.0.1:8080/v1/market/basis?symbols=BTCUSDT&exchanges=binance,okx" | jq
curl -s "http://127.0.0.1:8080/v1/market/order-flow?exchange=binance&market=perp&symbol=BTCUSDT&window_ms=60000" | jq
```

## Options Sources

Primary endpoints:

| Source | Data | Endpoint | Key |
|---|---|---|---|
| Deribit | Option summaries, depth, IV/Greeks/OI where returned, WS ticker cache updates. | `/v1/options/chains`, `/options/deribit/book` | keyless |
| OKX Options | Option summaries, depth, IV/Greeks where returned, WS summary cache updates. | `/v1/options/chains`, `/options/okx/book` | keyless |
| Bybit Options | Option tickers, depth, IV/Greeks/OI where returned, WS ticker cache updates. | `/v1/options/chains`, `/options/bybit/book` | keyless |
| Binance Options | European option tickers, mark/IV/Greeks where returned, depth, WS ticker/mark cache updates. | `/v1/options/chains`, `/options/binance/book` | keyless |

Native option WS book/trade parity remains a latency extension. REST depth plus
WS ticker/summary updates are already enough for research and paper decisions.

Example:

```bash
curl -s "http://127.0.0.1:8080/v1/options/chains?venue=deribit&currency=BTC" | jq
curl -s "http://127.0.0.1:8080/options/binance/book?instrument_name=BTC-260626-140000-C&depth=10" | jq
```

## Polymarket Sources

All current Polymarket data paths are public and keyless.

| Data | Endpoint | Key |
|---|---|---|
| Gamma market discovery | `/polymarket/markets`, `/polymarket/crypto-markets` | keyless |
| REST CLOB book | `/polymarket/book`, `/polymarket/books` | keyless |
| Live CLOB cache | `/polymarket/live-books`, `/v1/prediction/books` | keyless |
| Midpoints | `/polymarket/midpoints` | keyless |
| Spreads | `/polymarket/spreads` | keyless |
| Executable BUY/SELL prices | `/polymarket/prices` | keyless |
| Last trade prices | `/polymarket/last-trade-prices` | keyless |
| Historical CLOB prices/OHLCV | `/polymarket/prices-history` | keyless |

Authenticated Polymarket execution is intentionally out of scope for
MarketBridge.

## DeFi Sources

Primary quote endpoint:

```bash
curl -s "http://127.0.0.1:8080/v1/market/quotes?exchanges=jupiter,raydium,uniswap_v3,paraswap,oneinch" | jq
```

Native state metrics are emitted as `external_signal` rows:

```bash
curl -s "http://127.0.0.1:8080/v1/external/signals?sources=meteora,orca,uniswap_v3,pancakeswap,curve" | jq
```

| Source | Quote | Native state currently emitted | Key |
|---|---:|---|---|
| Jupiter | implemented | Route quote only; route-depth curves are future work. | keyless |
| Raydium | implemented | Token price map; protocol-native pool state is future work. | keyless |
| Uniswap V3 | implemented | Pool liquidity, TVL, volume, txCount from subgraph. | keyless by default subgraph |
| ParaSwap | implemented | Route quote only. | keyless by default base URL |
| 1inch | implemented configurable | Route quote only; newer gateways may need a different `base_url` or provider key. | keyless/config-dependent |
| Meteora / Orca / PancakeSwap / Balancer / Curve / SushiSwap / QuickSwap / Trader Joe / ETCSwap | implemented | DexScreener-backed liquidity, volume, and swap-count metrics. | keyless |

## Aggregate, Macro, And Sentiment Sources

| Source | Data | Endpoint | Key | Env var |
|---|---|---|---|---|
| CoinGecko | Crypto reference prices | `/v1/market/quotes?exchanges=coingecko` | optional | `COINGECKO_API_KEY` |
| CoinCap | Crypto reference prices | `/v1/market/quotes?exchanges=coincap` | optional | `COINCAP_API_KEY` |
| CoinMarketCap | Crypto reference prices | `/v1/market/quotes?exchanges=coinmarketcap` | required | `COINMARKETCAP_API_KEY` |
| CoinGlass | Funding/OI/liquidation/long-short/basis/options aggregate metrics | `/v1/external/signals?sources=coinglass` | required | `COINGLASS_API_KEY` |
| Fear & Greed | Crypto sentiment index | `/v1/external/signals?sources=fear_greed` | keyless | n/a |
| CryptoPanic | News items | `/v1/external/signals?sources=cryptopanic` | required | `CRYPTOPANIC_API_KEY` |
| Santiment | Social/on-chain metrics | `/v1/external/signals?sources=santiment` | required | `SANTIMENT_API_KEY` |
| LunarCrush | Social metrics | `/v1/external/signals?sources=lunarcrush` | required | `LUNARCRUSH_API_KEY` |
| DXY | Dollar index reference | `/v1/market/quotes?exchanges=dxy` | keyless | n/a |
| VIX | Volatility index reference | `/v1/market/quotes?exchanges=vix` | keyless | n/a |
| US10Y | 10-year treasury yield | `/v1/market/quotes?exchanges=us10y` | required | `FRED_API_KEY` |
| Custom API | User-defined numeric/JSON signals | `/v1/external/signals` | config-dependent | configured per source |

## On-Chain Transfer Sources

| Source | Scope | Endpoint | Key | Env var |
|---|---|---|---|---|
| Whale Alert | Global large transfers | `/v1/onchain/transfers?source=whale_alert` | required | `WHALE_ALERT_API_KEY` |
| mempool.space | Recent BTC mempool transactions | `/v1/onchain/transfers?source=mempool_space&chain=bitcoin` | keyless | n/a |
| Etherscan | Watched Ethereum addresses | `/v1/onchain/transfers?source=etherscan&chain=ethereum` | required | `ETHERSCAN_API_KEY` |

Etherscan is intentionally address-watchlist based and waits for configured safe
confirmations. It is not a full-chain Ethereum firehose.

## Streaming Usage

Subscribe only to the domains you need:

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=market_quote,trade,order_book&symbols=BTCUSDT&product_type=perp"
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=funding,open_interest,liquidation"
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=options_chain,prediction_book&include_stale=false"
```

Supported domains:

- `market_quote`
- `funding`
- `open_interest`
- `trade`
- `liquidation`
- `order_book`
- `external_signal`
- `options_chain`
- `prediction_book`

## Related Documents

- [`feature_inventory.md`](feature_inventory.md): implementation matrix and remaining intentional gaps.
- [`data_interfaces.md`](data_interfaces.md): endpoint-oriented consumer guide.
- [`architecture.md`](architecture.md): architecture and runtime design.
- [`ccxt_parity_audit.md`](ccxt_parity_audit.md): CCXT/Hummingbot reference audit; MarketBridge does not depend on them at runtime.
