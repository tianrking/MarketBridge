# MarketBridge Query Examples

This is a copy-paste cookbook for common MarketBridge searches. It focuses on
what users usually want to find: coins, symbols, exchange coverage, perpetual
contracts, funding rates, price changes, basis, liquidity, order flow,
liquidations, options, prediction markets, DeFi, macro references, on-chain
flows, exports, and health checks.

These examples assume MarketBridge is already running locally:

```bash
MB="http://127.0.0.1:8080"
curl -s "$MB/health" | jq
curl -s "$MB/v1/system/info" | jq
```

If API auth is enabled, add the header:

```bash
export MARKETBRIDGE_API_KEY="your-key"
AUTH=(-H "x-api-key: $MARKETBRIDGE_API_KEY")
curl "${AUTH[@]}" -s "$MB/health" | jq
curl "${AUTH[@]}" -s "$MB/v1/system/info" | jq
```

Notes:

- `funding_rate_pct` is already a percent value. `-0.2` means `-0.2%`.
- `limit=50000` is useful for broad discovery. Use smaller limits for UI pages.
- Always inspect `errors[]` on on-demand multi-exchange endpoints.
- These commands are data filters, not trading signals.

## Catalog And Symbol Discovery

### Q001. List supported runtime sources

```bash
curl -s "$MB/v1/catalog/sources" | jq
```

### Q002. List normalized data domains

```bash
curl -s "$MB/v1/catalog/domains" | jq
```

### Q003. List instruments currently visible in live caches

```bash
curl -s "$MB/v1/catalog/instruments" | jq
```

### Q004. Show catalog health and freshness

```bash
curl -s "$MB/v1/catalog/health" | jq
```

### Q005. List Binance USDT perpetual contracts

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq '.exchanges[0].contracts[] | {exchange, symbol, native_symbol, base, quote, active}'
```

### Q006. List OKX USDT perpetual contracts

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=okx&quote=USDT&limit=50000" \
| jq '.exchanges[0].contracts[] | {exchange, symbol, native_symbol, base, quote, active}'
```

### Q007. Count USDT perpetual contracts per exchange

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.exchanges[] | {exchange, contracts_total, contracts_returned, base_assets_total}'
```

### Q008. Find BTC perpetual listings across exchanges

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&base=BTC&limit=50000" \
| jq '.exchanges[] | {exchange, contracts: [.contracts[] | {symbol, native_symbol, quote, settle_asset}]}'
```

### Q009. Find ETH perpetual listings across exchanges

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&base=ETH&limit=50000" \
| jq '.exchanges[] | {exchange, contracts: [.contracts[] | {symbol, native_symbol, quote, settle_asset}]}'
```

### Q010. Find whether AERGO is listed as a USDT perpetual

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&base=AERGO&quote=USDT&limit=50000" \
| jq '.exchanges[] | {exchange, contracts_total, contracts: [.contracts[] | {symbol, native_symbol, active}]}'
```

### Q011. List Binance spot USDT markets

```bash
curl -s "$MB/v1/catalog/markets?exchange=binance&market=spot&quote=USDT&limit=100" \
| jq '.markets[] | {exchange, market, symbol, base, quote, active}'
```

### Q012. List Binance perpetual USDT markets

```bash
curl -s "$MB/v1/catalog/markets?exchange=binance&market=perp&quote=USDT&limit=100" \
| jq '.markets[] | {exchange, market, symbol, base, quote, active, contract_type}'
```

### Q013. Compare spot and perpetual markets for BTC on Binance

```bash
curl -s "$MB/v1/catalog/markets?exchange=binance&base=BTC&limit=50000" \
| jq '.markets[] | {market, symbol, native_symbol, quote, active, contract_type}'
```

### Q014. Show all base assets listed as Binance USDT perpetuals

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.exchanges[0].base_assets[]'
```

### Q015. Generate a Binance USDT perpetual watchlist

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.exchanges[0].contracts[].symbol' > watchlist_binance_usdt_perps.txt
```

## Funding Rate Searches

### Q016. Query Binance current USDT perpetual funding

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding[] | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

### Q017. Query Bybit current USDT perpetual funding

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=bybit&quote=USDT&limit=50000" \
| jq '.funding[] | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

### Q018. Check adapter errors for multi-exchange funding

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '{rows:(.funding | length), errors}'
```

### Q019. Find Binance funding between -2% and -0.2%

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

### Q020. Find Binance funding below -0.1%

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding | map(select(.funding_rate_pct <= -0.1)) | sort_by(.funding_rate_pct) | .[]'
```

### Q021. Find Binance funding above 0.2%

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding | map(select(.funding_rate_pct >= 0.2)) | sort_by(-.funding_rate_pct) | .[]'
```

### Q022. Find near-neutral Binance funding

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -0.005 and .funding_rate_pct <= 0.005))
  | sort_by(.symbol)
  | .[] | {symbol, funding_rate_pct, mark_price}'
```

### Q023. Find top 20 most negative funding rows

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding | sort_by(.funding_rate_pct) | .[:20] | .[] | {exchange, symbol, funding_rate_pct, mark_price}'
```

### Q024. Find top 20 most positive funding rows

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding | sort_by(-.funding_rate_pct) | .[:20] | .[] | {exchange, symbol, funding_rate_pct, mark_price}'
```

### Q025. Group extreme negative funding by exchange

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct <= -0.2))
  | group_by(.exchange)
  | map({exchange: .[0].exchange, count: length, rows: (sort_by(.funding_rate_pct) | map({symbol, funding_rate_pct, mark_price}))})'
```

### Q026. Group extreme positive funding by exchange

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= 0.2))
  | group_by(.exchange)
  | map({exchange: .[0].exchange, count: length, rows: (sort_by(-.funding_rate_pct) | map({symbol, funding_rate_pct, mark_price}))})'
```

### Q027. Compare BTCUSDT funding across exchanges

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=BTCUSDT&limit=50000" \
| jq '.funding | sort_by(.exchange) | .[] | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

### Q028. Compare ETHUSDT funding across exchanges

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=ETHUSDT&limit=50000" \
| jq '.funding | sort_by(.exchange) | .[] | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

### Q029. Compute cross-exchange funding spread per symbol

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | group_by(.symbol)
  | map(select(length >= 2)
    | {symbol: .[0].symbol,
       exchanges: (map(.exchange) | unique),
       min_pct: (map(.funding_rate_pct) | min),
       max_pct: (map(.funding_rate_pct) | max),
       spread_pct: ((map(.funding_rate_pct) | max) - (map(.funding_rate_pct) | min))})
  | sort_by(-.spread_pct)
  | .[:30]'
```

### Q030. Export extreme Binance negative funding to CSV

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '["exchange","symbol","funding_rate_pct","mark_price","next_funding_time_ms"],
  (.funding | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2)) | sort_by(.funding_rate_pct) | .[]
  | [.exchange, .symbol, .funding_rate_pct, .mark_price, .next_funding_time_ms]) | @csv' \
> binance_extreme_negative_funding.csv
```

## Quotes, Price, And Price-Change Searches

### Q031. Query BTCUSDT perpetual quote

```bash
curl -s "$MB/v1/market/quotes?symbols=BTCUSDT&product_type=perp" | jq
```

### Q032. Query BTCUSDT and ETHUSDT perpetual quotes

```bash
curl -s "$MB/v1/market/quotes?symbols=BTCUSDT,ETHUSDT&product_type=perp" | jq
```

### Q033. Query Binance and OKX BTCUSDT quotes

```bash
curl -s "$MB/v1/market/quotes?symbols=BTCUSDT&exchanges=binance,okx&product_type=perp" | jq
```

### Q034. Query spot quotes for BTCUSDT

```bash
curl -s "$MB/v1/market/quotes?symbols=BTCUSDT&product_type=spot" | jq
```

### Q035. Query latest normalized snapshots

```bash
curl -s "$MB/snapshot?symbol=BTCUSDT" | jq
```

### Q036. Find top 1-day percent gainers

```bash
curl -s "$MB/v1/universe/percent-change?interval=1d&limit=50" | jq
```

### Q037. Find top 1-day movers by volatility

```bash
curl -s "$MB/v1/universe/volatility?interval=1d&limit=50" | jq
```

### Q038. Find high-volume Binance perpetual symbols

```bash
curl -s "$MB/v1/universe/top-volume?exchange=binance&market=perp&interval=1d&limit=50" | jq
```

### Q039. Find low-spread perpetual symbols

```bash
curl -s "$MB/v1/universe/spread-filter?product_type=perp&max_spread_bps=5" | jq
```

### Q040. Find symbols available on both spot and perp

```bash
curl -s "$MB/v1/universe/cross-market?require_both=true" | jq
```

### Q041. Find recently listed symbols

```bash
curl -s "$MB/v1/universe/new-listings?max_age_days=7" | jq
```

### Q042. Find symbols with missing or stale quote risk

```bash
curl -s "$MB/v1/universe/delist-risk?stale_after_ms=86400000" | jq
```

### Q043. Query 1-minute Binance BTCUSDT perpetual klines

```bash
curl -s "$MB/v1/market/klines?exchange=binance&market=perp&symbol=BTCUSDT&interval=1m&limit=100" | jq
```

### Q044. Query historical Binance mark-price candles

```bash
curl -s "$MB/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=mark&interval=1m&limit=500" | jq
```

### Q045. Query historical OKX funding-rate candles

```bash
curl -s "$MB/v1/history/candles?exchange=okx&symbol=BTCUSDT&candle_type=funding_rate&limit=100" | jq
```

## Basis, Open Interest, And Derivatives Context

### Q046. Query BTCUSDT spot-perp basis

```bash
curl -s "$MB/v1/market/basis?symbols=BTCUSDT&exchanges=binance,okx" | jq
```

### Q047. Query BTCUSDT and ETHUSDT basis

```bash
curl -s "$MB/v1/market/basis?symbols=BTCUSDT,ETHUSDT&exchanges=binance,okx,bybit" | jq
```

### Q048. Query latest funding rows from live caches

```bash
curl -s "$MB/v1/market/funding?symbols=BTCUSDT&exchanges=binance,okx,bybit" | jq
```

### Q049. Query latest open interest for BTCUSDT

```bash
curl -s "$MB/v1/market/open-interest?symbols=BTCUSDT&exchanges=binance,okx,bybit" | jq
```

### Q050. Query OI for several symbols

```bash
curl -s "$MB/v1/market/open-interest?symbols=BTCUSDT,ETHUSDT,SOLUSDT&exchanges=binance,okx,bybit" | jq
```

### Q051. Query legacy unified funding view

```bash
curl -s "$MB/funding?symbols=BTCUSDT&exchanges=okx,bybit,bitget" | jq
```

### Q052. Query coverage for BTCUSDT perp

```bash
curl -s "$MB/coverage?market=perp&symbols=BTCUSDT" | jq
```

### Q053. Query coverage by exchanges

```bash
curl -s "$MB/coverage?market=perp&exchanges=okx,bybit,bitget" | jq
```

### Q054. Query market regime for BTC and ETH

```bash
curl -s "$MB/v1/research/market-regime?symbols=BTCUSDT,ETHUSDT&intervals=1h,4h" | jq
```

### Q055. Query research features for BTC and ETH

```bash
curl -s "$MB/v1/research/features?symbols=BTCUSDT,ETHUSDT&intervals=1h,4h,1d&benchmark_symbol=BTCUSDT&correlated_symbols=ETHUSDT,SOLUSDT" | jq
```

## Order Books, Trades, Order Flow, And Liquidations

### Q056. Query Binance BTCUSDT perpetual order book

```bash
curl -s "$MB/v1/market/order-books?symbols=BTCUSDT&market=perp&exchanges=binance" | jq
```

### Q057. Query OKX BTCUSDT perpetual order book

```bash
curl -s "$MB/v1/market/order-books?symbols=BTCUSDT&market=perp&exchanges=okx" | jq
```

### Q058. Query BTCUSDT recent trades

```bash
curl -s "$MB/v1/market/trades?symbols=BTCUSDT&market=perp&exchanges=binance,okx" | jq
```

### Q059. Query BTCUSDT one-minute order flow

```bash
curl -s "$MB/v1/market/order-flow?exchange=binance&market=perp&symbol=BTCUSDT&window_ms=60000&limit=50" | jq
```

### Q060. Query BTCUSDT multi-window order flow

```bash
curl -s "$MB/v1/market/order-flow/windows?exchange=binance&market=perp&symbol=BTCUSDT&windows_ms=60000,300000,900000" | jq
```

### Q061. Query BTCUSDT footprint profile

```bash
curl -s "$MB/v1/market/footprint?exchange=binance&market=perp&symbol=BTCUSDT&interval_ms=60000&scale=1" | jq
```

### Q062. Query footprint with raw trades omitted

```bash
curl -s "$MB/v1/market/footprint?exchange=binance&market=perp&symbol=BTCUSDT&interval_ms=60000&scale=1&include_trades=false" | jq
```

### Q063. Query recent liquidations for BTCUSDT

```bash
curl -s "$MB/v1/market/liquidations?symbols=BTCUSDT&exchanges=binance,bybit,okx" | jq
```

### Q064. Query recent liquidations for multiple symbols

```bash
curl -s "$MB/v1/market/liquidations?symbols=BTCUSDT,ETHUSDT,SOLUSDT&exchanges=binance,bybit,okx" | jq
```

### Q065. Query symbol-state read-only analysis

```bash
curl -s "$MB/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance" | jq
```

## Options And Volatility

### Q066. Query Bybit BTC call option chain

```bash
curl -s "$MB/v1/options/chains?venue=bybit&currency=BTC&option_type=call" | jq
```

### Q067. Query Deribit BTC option chain

```bash
curl -s "$MB/v1/options/chains?venue=deribit&currency=BTC" | jq
```

### Q068. Query OKX BTC call options above a strike range

```bash
curl -s "$MB/v1/options/chains?venue=okx&currency=BTC&option_type=call&strike_min=90000&strike_max=120000" | jq
```

### Q069. Query Deribit summary

```bash
curl -s "$MB/options/deribit/summary?currency=BTC" | jq
```

### Q070. Query Deribit live summary

```bash
curl -s "$MB/options/deribit/live-summary?currency=BTC&option_type=call&strike_min=90000&strike_max=120000" | jq
```

### Q071. Query Deribit option book

```bash
curl -s "$MB/options/deribit/book?instrument_name=BTC-29MAY26-70000-P&depth=10" | jq
```

### Q072. Query OKX option book

```bash
curl -s "$MB/options/okx/book?instrument_name=BTC-USD-260626-100000-C&depth=10" | jq
```

### Q073. Query Bybit option book

```bash
curl -s "$MB/options/bybit/book?instrument_name=BTC-26MAR27-78000-P-USDT&depth=10" | jq
```

### Q074. Query Binance option book

```bash
curl -s "$MB/options/binance/book?instrument_name=BTC-260626-140000-C&depth=10" | jq
```

### Q075. Subscribe to option-chain snapshots

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=options_chain&include_stale=false"
```

## Polymarket And Prediction Markets

### Q076. Discover parsed Polymarket BTC/ETH markets

```bash
curl -s "$MB/polymarket/crypto-markets?limit=500&max_offset=500" | jq
```

### Q076A. Discover all active Polymarket Gamma markets

```bash
curl -s "$MB/polymarket/markets?limit=500&max_offset=500" | jq
```

### Q077. Query one Polymarket token book

```bash
curl -s "$MB/polymarket/book?token_id=TOKEN_ID" | jq
```

### Q078. Query several Polymarket token books

```bash
curl -s "$MB/polymarket/books?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### Q079. Query Polymarket midpoints

```bash
curl -s "$MB/polymarket/midpoints?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### Q080. Query Polymarket spreads

```bash
curl -s "$MB/polymarket/spreads?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### Q081. Query Polymarket last trade prices

```bash
curl -s "$MB/polymarket/last-trade-prices?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### Q082. Query Polymarket executable prices

```bash
curl -s "$MB/polymarket/prices?token_ids=YES_TOKEN&sides=BUY,SELL" | jq
```

### Q083. Query Polymarket price history

```bash
curl -s "$MB/polymarket/prices-history?token_id=YES_TOKEN&interval=1h&fidelity=1" | jq
```

### Q084. Query live cached Polymarket books

```bash
curl -s "$MB/polymarket/live-books?token_ids=YES_TOKEN,NO_TOKEN" | jq
```

### Q085. Query v1 prediction books

```bash
curl -s "$MB/v1/prediction/books?token_ids=YES_TOKEN,NO_TOKEN&include_stale=false" | jq
```

## DeFi, Macro, Aggregates, Sentiment, And On-Chain

### Q086. Query DeFi quote sources

```bash
curl -s "$MB/v1/market/quotes?exchanges=jupiter,raydium,uniswap_v3,paraswap,oneinch" | jq
```

### Q087. Query DXY, VIX, and US10Y references

```bash
curl -s "$MB/v1/market/quotes?exchanges=dxy,vix,us10y" | jq
```

### Q088. Query aggregate and sentiment signals

```bash
curl -s "$MB/v1/external/signals?sources=coinglass,fear_greed,cryptopanic,santiment,lunarcrush" | jq
```

### Q089. Query CoinGlass funding and open-interest signals

```bash
curl -s "$MB/v1/external/signals?sources=coinglass&symbols=BTC&metrics=funding,open_interest" | jq
```

### Q090. Query Whale Alert large transfers

```bash
curl -s "$MB/v1/onchain/transfers?source=whale_alert&asset=BTC&min_amount_usd=1000000" | jq
```

### Q091. Query mempool.space BTC transfers

```bash
curl -s "$MB/v1/onchain/transfers?source=mempool_space&chain=bitcoin&asset=BTC" | jq
```

### Q092. Query Etherscan watched-address transfers

```bash
curl -s "$MB/v1/onchain/transfers?source=etherscan&chain=ethereum&asset=ETH" | jq
```

### Q093. Query market-cap ranking

```bash
curl -s "$MB/v1/universe/market-cap?limit=100" | jq
```

### Q094. Query age-filtered symbols

```bash
curl -s "$MB/v1/universe/age-filter?max_age_days=30" | jq
```

### Q095. Query new listings

```bash
curl -s "$MB/v1/universe/new-listings?max_age_days=7" | jq
```

## WebSocket Streams

### Q096. Stream BTCUSDT perpetual market quotes

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=market_quote&symbols=BTCUSDT&product_type=perp"
```

### Q097. Stream BTCUSDT funding updates

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=funding&symbols=BTCUSDT&exchanges=binance,okx"
```

### Q098. Stream BTCUSDT order books and trades

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=order_book,trade&symbols=BTCUSDT&product_type=perp"
```

### Q099. Stream options and prediction-book snapshots

```bash
wscat -c "ws://127.0.0.1:8080/v1/stream?domains=options_chain,prediction_book&include_stale=false"
```

### Q100. Use the legacy tick stream

```bash
wscat -c "ws://127.0.0.1:8080/ws/ticks?market=perp&symbols=BTCUSDT"
```

## Storage, Exports, And Operations

### Q101. Persist Binance BTCUSDT klines to the local lake

```bash
curl -s "$MB/v1/market/klines?exchange=binance&market=perp&symbol=BTCUSDT&interval=1m&limit=1000&persist=true" | jq
```

### Q102. Persist historical mark candles to the local lake

```bash
curl -s "$MB/v1/history/candles?exchange=binance&symbol=BTCUSDT&candle_type=mark&interval=1m&limit=1000&persist=true" | jq
```

### Q103. Inspect the local lake manifest

```bash
curl -s "$MB/v1/storage/manifest?domain=candles&symbol=BTCUSDT" | jq
```

### Q104. Delete selected candle partitions

```bash
curl -X DELETE "$MB/v1/storage/partitions?domain=candles&exchange=binance&symbol=BTCUSDT&interval=1m&candle_type=mark" | jq
```

### Q105. Export all Binance USDT perpetual symbols to text

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.exchanges[0].contracts[].symbol' > binance_usdt_perps.txt
```

### Q106. Export multi-exchange extreme funding to CSV

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq -r '["exchange","symbol","funding_rate_pct","mark_price","next_funding_time_ms"],
  (.funding | map(select(.funding_rate_pct <= -0.2 or .funding_rate_pct >= 0.2)) | sort_by(.funding_rate_pct) | .[]
  | [.exchange, .symbol, .funding_rate_pct, .mark_price, .next_funding_time_ms]) | @csv' \
> extreme_funding.csv
```

### Q107. Export cross-exchange funding spread to JSON

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | group_by(.symbol)
  | map(select(length >= 2)
    | {symbol: .[0].symbol,
       exchanges: (map(.exchange) | unique),
       min_pct: (map(.funding_rate_pct) | min),
       max_pct: (map(.funding_rate_pct) | max),
       spread_pct: ((map(.funding_rate_pct) | max) - (map(.funding_rate_pct) | min))})
  | sort_by(-.spread_pct)' > funding_spreads.json
```

### Q108. Generate a negative-funding watchlist

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.funding | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2)) | sort_by(.funding_rate_pct) | .[].symbol' \
> watchlist_binance_negative_funding.txt
```

### Q109. Check Prometheus metrics

```bash
curl -s "$MB/metrics"
```

### Q110. Run a local synthetic load test

```bash
./market-bridge load-test --events 100000 --subscribers 8 --broadcast-capacity 65536 --event-bus-shards 1
```

### Q111. Check source availability after startup

```bash
curl -s "$MB/v1/catalog/sources" | jq '.sources[] | {source, status, key_state}'
```

### Q112. Check whether an endpoint returned partial data

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.errors'
```

### Q113. Count returned funding rows

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding | length'
```

### Q114. Convert funding time to UTC

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct,
     next_funding_utc: (if .next_funding_time_ms then ((.next_funding_time_ms / 1000) | strftime("%Y-%m-%d %H:%M:%S UTC")) else null end)}'
```

### Q115. Compare contract discovery and funding coverage

```bash
comm -23 \
  <(curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" | jq -r '.exchanges[0].contracts[].symbol' | sort) \
  <(curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" | jq -r '.funding[].symbol' | sort)
```

### Q116. Show all available docs from a release package

```bash
find docs -maxdepth 1 -type f -name '*.md' -printf '%f\n' | sort
```

### Q117. Check root metadata

```bash
curl -s "$MB/" | jq
```

### Q118. Build an API context bundle for an agent

```bash
curl -s "$MB/v1/agent/context?symbols=BTCUSDT,ETHUSDT&include_storage=true" | jq
```

### Q119. List agent capabilities

```bash
curl -s "$MB/v1/agent/capabilities" | jq
```

### Q120. Download release binaries

```bash
open "https://github.com/tianrking/MarketBridge/releases/latest"
```
