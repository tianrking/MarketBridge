# MarketBridge Full Usage Guide

MarketBridge is a data-only market data layer. It collects public data,
normalizes it, computes research-friendly derived views, and can persist only
the historical data you explicitly ask it to keep.

It does not place orders, manage wallets, custody funds, or claim strategy
profitability.

## Run

```bash
MARKETBRIDGE_CONFIG=./config.yaml cargo run
```

Health check:

```bash
curl -s http://127.0.0.1:8080/health | jq
```

## Storage Model

MarketBridge has two local storage roles:

- SQLite: kline working store plus lake manifest/index.
- Parquet lake: optional local persisted partitions for data you request with
  `persist=true`.

Nothing is written to the local lake unless a request opts in.

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

Lake partitions are organized by domain, exchange, market, symbol, interval,
candle type, and UTC day. Manifest rows include row count, file size, first/last
timestamp, latest watermark, gap count, duplicate count, coverage ratio,
latency percentiles, and stale count.

## Current Snapshots

Quotes:

```bash
curl -s "http://127.0.0.1:8080/v1/market/quotes?symbols=BTCUSDT&product_type=perp" | jq
```

Funding and open interest:

```bash
curl -s "http://127.0.0.1:8080/v1/market/funding?symbols=BTCUSDT" | jq
curl -s "http://127.0.0.1:8080/v1/market/open-interest?symbols=BTCUSDT" | jq
```

## Discover Perpetual Contracts And Funding

Use these endpoints when a client needs the latest exchange universe instead of
only symbols configured in `config.yaml`. MarketBridge returns raw normalized
data; clients own threshold filters, watchlists, monitoring, and alerting.

List all USDT perpetual contracts for one exchange:

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/perpetuals?exchange=bybit&quote=USDT&limit=50" | jq
```

List grouped perpetual contracts across several exchanges:

```bash
curl -s "http://127.0.0.1:8080/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=10" | jq
```

Query current on-demand funding rows:

```bash
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchange=bybit&quote=USDT&limit=50000" | jq
```

Filter extreme negative funding in the client, for example Binance contracts
between `-2%` and `-0.2%`:

```bash
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

For a multi-exchange version:

```bash
curl -s "http://127.0.0.1:8080/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Important response fields:

- `/v1/catalog/perpetuals`: `exchanges[].contracts_total`,
  `exchanges[].base_assets`, `exchanges[].contracts[]`.
- `/v1/market/perpetual-funding`: `funding[].funding_rate`,
  `funding[].funding_rate_pct`, `funding[].mark_price`,
  `funding[].next_funding_time_ms`.
- `funding_rate_pct` is already percent; `-0.1` means `-0.1%`.
- Non-empty `errors[]` means one or more exchange adapters failed and the
  result should be treated as partial.

Books and trades:

```bash
curl -s "http://127.0.0.1:8080/v1/market/order-books?symbols=BTCUSDT&market=perp" | jq
curl -s "http://127.0.0.1:8080/v1/market/trades?symbols=BTCUSDT&market=perp" | jq
```

Basis:

```bash
curl -s "http://127.0.0.1:8080/v1/market/basis?symbols=BTCUSDT" | jq
```

## Historical Candles

Use `/v1/history/candles` for on-demand historical candle retrieval. Add
`persist=true` only when you want the result written to the local Parquet lake.

Supported candle types:

- Binance: `spot`, `futures`, `perp`, `mark`, `index`, `premiumIndex`,
  `funding_rate`
- OKX: `spot`, `perp`, `mark`, `index`, `funding_rate`

Examples:

```bash
curl -s "http://127.0.0.1:8080/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=mark&interval=1m&limit=1000&persist=true" | jq
```

```bash
curl -s "http://127.0.0.1:8080/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=premiumIndex&interval=1m&limit=500&persist=true" | jq
```

```bash
curl -s "http://127.0.0.1:8080/v1/history/candles?exchange=okx&symbol=BTCUSDT&candle_type=funding_rate&limit=100&persist=true" | jq
```

## Local Lake Manifest And Delete

List local partitions:

```bash
curl -s "http://127.0.0.1:8080/v1/storage/manifest?domain=candles&symbol=BTCUSDT" | jq
```

Delete local partitions by filter:

```bash
curl -X DELETE "http://127.0.0.1:8080/v1/storage/partitions?domain=candles&exchange=binance&symbol=BTCUSDT&interval=1m&candle_type=mark" | jq
```

Deletion refuses an empty filter to avoid accidental full-lake removal.

## Orderflow Pro

Order-flow windows:

```bash
curl -s "http://127.0.0.1:8080/v1/market/order-flow/windows?exchange=binance&market=perp&symbol=BTCUSDT&windows_ms=60000,300000,900000" | jq
```

Footprint:

```bash
curl -s "http://127.0.0.1:8080/v1/market/footprint?exchange=binance&market=perp&symbol=BTCUSDT&interval_ms=60000&scale=1&include_trades=false" | jq
```

Returned metrics include buy/sell amount, notional, delta, CVD, aggressive
buy/sell ratio, price-bin volume profile, per-price delta, imbalance, stacked
imbalance, min/max delta, and total trades.

## Universe Engine

```bash
curl -s "http://127.0.0.1:8080/v1/universe/top-volume?exchange=binance&market=perp&interval=1d&limit=50" | jq
curl -s "http://127.0.0.1:8080/v1/universe/percent-change?interval=1d&limit=50" | jq
curl -s "http://127.0.0.1:8080/v1/universe/volatility?interval=1d&limit=50" | jq
curl -s "http://127.0.0.1:8080/v1/universe/spread-filter?product_type=perp&max_spread_bps=5" | jq
curl -s "http://127.0.0.1:8080/v1/universe/cross-market?require_both=true" | jq
curl -s "http://127.0.0.1:8080/v1/universe/market-cap?limit=100" | jq
curl -s "http://127.0.0.1:8080/v1/universe/age-filter?max_age_days=30" | jq
curl -s "http://127.0.0.1:8080/v1/universe/new-listings?max_age_days=7" | jq
curl -s "http://127.0.0.1:8080/v1/universe/delist-risk?stale_after_ms=86400000" | jq
```

Universe endpoints are data filters, not trading signals.

## Research Features

```bash
curl -s "http://127.0.0.1:8080/v1/research/features?symbols=BTCUSDT,ETHUSDT&intervals=1h,4h,1d&benchmark_symbol=BTCUSDT&correlated_symbols=ETHUSDT,SOLUSDT" | jq
```

```bash
curl -s "http://127.0.0.1:8080/v1/research/market-regime?symbols=BTCUSDT,ETHUSDT&intervals=1h,4h" | jq
```

Returned features include rolling return, rolling volume, realized volatility,
z-score, benchmark correlation, correlated asset features, basis regime,
funding/OI regime, liquidity score, exchange disagreement, and market regime.

## Agent Mode

Agent mode is an AI-friendly read-only API surface. It gives an agent a compact
context bundle and tells it which follow-up endpoints are available.

Capabilities:

```bash
curl -s "http://127.0.0.1:8080/v1/agent/capabilities" | jq
```

Context:

```bash
curl -s "http://127.0.0.1:8080/v1/agent/context?symbols=BTCUSDT,ETHUSDT&include_storage=true" | jq
```

The agent contract is explicit: read-only market data, no order execution, no
wallets, no strategy guarantee.

## WebSocket

```bash
npx wscat -c "ws://127.0.0.1:8080/v1/stream?domains=market_quote,trade,order_book&symbols=BTCUSDT&product_type=perp&snapshot_interval_ms=250"
```

Supported domains include `market_quote`, `funding`, `open_interest`, `trade`,
`liquidation`, `order_book`, `external_signal`, `options_chain`, and
`prediction_book`.
