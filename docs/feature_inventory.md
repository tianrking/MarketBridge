# MarketBridge Feature Inventory

This document is the operating checklist for MarketBridge coverage. Update it in
the same commit as every connector, API, or infrastructure change.

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
| Funding rates | `/v1/market/funding` | implemented | keyless | Native CEX feeds plus future aggregate feeds. |
| Open interest | `/v1/market/open-interest` | implemented | keyless | Native CEX feeds plus future aggregate feeds. |
| Liquidations | `/v1/market/liquidations` | implemented | keyless | Native venue feeds where available. |
| Order books | `/v1/market/order-books` | implemented | keyless | Latest L2 snapshot per venue/symbol. |
| Trades | `/v1/market/trades` | implemented | keyless | Latest trade per venue/symbol. |
| Options chains | `/v1/options/chains` | implemented | keyless | Deribit/OKX/Bybit/Binance REST cache. |
| Prediction books | `/v1/prediction/books` | implemented | keyless | Polymarket live CLOB cache. |
| External signals | `/v1/external/signals` | implemented | mixed | CoinGlass, news, social, sentiment. |

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
| Bitget | implemented | planned | planned | planned | planned | planned | keyless | Only ticker today. |
| KuCoin | implemented | planned | planned | planned | planned | planned | keyless | Spot/perp ticker today. |
| Gate | implemented | planned | planned | planned | planned | planned | keyless | BookTicker-style BBO today. |
| Kraken | implemented | planned | planned | planned | planned | planned | keyless | Spot V2 ticker and separate futures ticker today. |
| HTX | implemented | planned | planned | planned | planned | planned | keyless | BBO only today. |
| Bitfinex | implemented | planned | planned | planned | planned | planned | keyless | Spot/perp ticker today. |
| Coinbase | implemented | planned | planned | n/a | n/a | n/a | keyless | Spot ticker only; Coinbase International not wired. |

## Polymarket Coverage

| Capability | Status | API key | Notes |
|---|---:|---:|---|
| Gamma market discovery | implemented | keyless | BTC/ETH crypto binary parser today. |
| REST book | implemented | keyless | `/polymarket/book`, `/polymarket/books`. |
| Live CLOB cache | implemented | keyless | `/polymarket/live-books`, `/v1/prediction/books`. |
| Midpoint batch | planned | keyless | CLOB `/midpoint` wrapper needed. |
| Last trade price batch | planned | keyless | CLOB `/last-trade-price` wrapper needed. |
| Spread batch | planned | keyless | CLOB `/spread` wrapper needed. |
| Market prices batch | planned | keyless | CLOB `/market-prices` wrapper needed. |
| Historical prices/OHLCV | planned | keyless | CLOB price history wrapper needed. |
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
| CoinMarketCap | market_quote | implemented | required | `COINMARKETCAP_API_KEY` |
| CoinGlass | external_signal | implemented | required | `COINGLASS_API_KEY` |
| Fear & Greed | external_signal | implemented | keyless | n/a |
| CryptoPanic | external_signal | implemented | required | `CRYPTOPANIC_API_KEY` |
| Santiment | external_signal | implemented | required | `SANTIMENT_API_KEY` |
| LunarCrush | external_signal | implemented | required | `LUNARCRUSH_API_KEY` |
| DXY | market_quote | implemented | keyless | n/a |
| VIX | market_quote | implemented | keyless | n/a |
| US10Y | market_quote | implemented | required | `FRED_API_KEY` |

## Infrastructure Gaps

| Capability | Status | Priority | Notes |
|---|---:|---:|---|
| Redis all event types | implemented | P0 | Writes quote/funding/OI/trade/book/liquidation/external_signal/heartbeat streams. |
| Extended EventBus broadcast | implemented | P1 | `subscribe_events()` broadcasts raw `DataEvent` in addition to quote ticks. |
| Order-book level arbitrage | planned | P1 | L2 data is exposed, but spread engine uses BBO only. |
| Maker fee modeling | planned | P2 | Fee config stores maker fee, spread engine uses taker only. |
| Dynamic catalog from runtime config | planned | P2 | Static catalog documents possible sources, not enabled sources. |
| Aggregator extended event analytics | planned | P1 | Funding/OI/book/trade/liquidation are stored by API but ignored by spread engine. |
