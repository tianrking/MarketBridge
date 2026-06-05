# Perpetual Contract And Funding-Rate Cookbook

This cookbook treats MarketBridge as a unified data API. MarketBridge returns
raw normalized data; `jq`, shell scripts, Python, or downstream services decide
which symbols to watch, how to filter them, and when to alert.

Start MarketBridge first:

```bash
cd /path/to/MarketBridge
MARKETBRIDGE_CONFIG=./config.min.yaml cargo run
```

In another terminal:

```bash
MB="http://127.0.0.1:8080"
curl -s "$MB/health" | jq
```

If API authentication is enabled:

```bash
export MARKETBRIDGE_API_KEY="your-key"
curl -H "x-api-key: $MARKETBRIDGE_API_KEY" -s "$MB/health" | jq
```

`funding_rate_pct` is already expressed in percent. For example, `-0.2` means
`-0.2%`, and `1.5` means `1.5%`.

## 1. Discover Perpetual Contracts On One Exchange

List the first 20 Binance USDT perpetual contracts:

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=20" \
| jq '.exchanges[0].contracts[]
  | {exchange, symbol, native_symbol, base, quote, active, status, contract_type}'
```

Show only the contract and base-asset counts:

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts_total, contracts_returned, base_assets_total}'
```

List USDT perpetual counts across several exchanges:

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts_total, base_assets_total}'
```

Find BTC perpetual contracts across exchanges:

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&base=BTC&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts: [.contracts[] | {symbol, native_symbol, quote, settle_asset}]}'
```

## 2. Query Current Funding Rows

Return simplified Binance USDT perpetual funding rows:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding[]
  | {exchange, symbol, funding_rate_pct, mark_price, index_price, next_funding_time_ms}'
```

Check row count and exchange-adapter errors:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '{rows:(.funding | length), errors}'
```

Summarize row count, minimum funding, and maximum funding per exchange:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | group_by(.exchange)
  | map({
      exchange: .[0].exchange,
      rows: length,
      min_pct: (map(.funding_rate_pct) | min),
      max_pct: (map(.funding_rate_pct) | max)
    })'
```

## 3. Find Binance Funding Between -2% And -0.2%

This is a common extreme negative funding search:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Return only symbols for a watchlist:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[].symbol'
```

## 4. Search Extreme Negative Funding Across Exchanges

Search Binance, OKX, Bybit, and Bitget for `-2%` to `-0.2%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Group the results by exchange:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | group_by(.exchange)
  | map({
      exchange: .[0].exchange,
      count: length,
      rows: (sort_by(.funding_rate_pct)
        | map({symbol, funding_rate_pct, mark_price, next_funding_time_ms}))
    })'
```

## 5. Find The Most Negative Funding Rates

Show the 20 lowest funding rates without a lower bound:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | sort_by(.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Show the 20 lowest rates at or below `-0.1%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct <= -0.1))
  | sort_by(.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 6. Find Extreme Positive Funding

High positive funding can indicate crowded longs. Search `0.2%` to `2%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= 0.2 and .funding_rate_pct <= 2))
  | sort_by(-.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Show the top 20 positive funding rows:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | sort_by(-.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 7. Find Near-Neutral Funding

Search Binance contracts with funding between `-0.005%` and `0.005%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -0.005 and .funding_rate_pct <= 0.005))
  | sort_by(.symbol)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 8. Compare One Symbol Across Exchanges

Compare BTCUSDT and ETHUSDT across exchanges:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=BTCUSDT,ETHUSDT&limit=50000" \
| jq '.funding
  | sort_by(.symbol, .exchange)
  | .[]
  | {symbol, exchange, funding_rate_pct, mark_price, next_funding_time_ms}'
```

Check where a smaller contract such as AERGOUSDT has funding data:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=AERGOUSDT&limit=50000" \
| jq '.funding
  | sort_by(.funding_rate_pct)
  | .[]
  | {symbol, exchange, funding_rate_pct, mark_price, source}'
```

## 9. Compute Cross-Exchange Funding Spreads

Find symbols listed on at least two exchanges and compute max-minus-min funding:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | group_by(.symbol)
  | map(select(length >= 2)
    | {
        symbol: .[0].symbol,
        exchanges: (map(.exchange) | unique),
        min_pct: (map(.funding_rate_pct) | min),
        max_pct: (map(.funding_rate_pct) | max),
        spread_pct: ((map(.funding_rate_pct) | max) - (map(.funding_rate_pct) | min))
      })
  | sort_by(-.spread_pct)
  | .[:30]'
```

Show only symbols whose exchange funding spread is at least `0.2%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | group_by(.symbol)
  | map(select(length >= 2)
    | {
        symbol: .[0].symbol,
        exchanges: (map(.exchange) | unique),
        min_pct: (map(.funding_rate_pct) | min),
        max_pct: (map(.funding_rate_pct) | max),
        spread_pct: ((map(.funding_rate_pct) | max) - (map(.funding_rate_pct) | min))
      })
  | map(select(.spread_pct >= 0.2))
  | sort_by(-.spread_pct)'
```

## 10. Export CSV Files

Export Binance contracts with funding between `-2%` and `-0.2%`:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '
  ["exchange","symbol","funding_rate_pct","mark_price","next_funding_time_ms"],
  (.funding
    | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
    | sort_by(.funding_rate_pct)
    | .[]
    | [.exchange, .symbol, .funding_rate_pct, .mark_price, .next_funding_time_ms])
  | @csv' \
> binance_extreme_negative_funding.csv
```

Export both positive and negative extreme funding across several exchanges:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq -r '
  ["exchange","symbol","funding_rate_pct","mark_price","next_funding_time_ms"],
  (.funding
    | map(select(.funding_rate_pct <= -0.2 or .funding_rate_pct >= 0.2))
    | sort_by(.funding_rate_pct)
    | .[]
    | [.exchange, .symbol, .funding_rate_pct, .mark_price, .next_funding_time_ms])
  | @csv' \
> extreme_funding.csv
```

## 11. Convert Funding Timestamps To UTC

`next_funding_time_ms` is Unix milliseconds:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {
      exchange,
      symbol,
      funding_rate_pct,
      next_funding_utc: (
        if .next_funding_time_ms
        then ((.next_funding_time_ms / 1000) | strftime("%Y-%m-%d %H:%M:%S UTC"))
        else null
        end
      )
    }'
```

## 12. Find Contracts Missing Funding Rows

This helps check whether discovery and funding coverage align for an exchange:

```bash
comm -23 \
  <(curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
    | jq -r '.exchanges[0].contracts[].symbol' | sort) \
  <(curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
    | jq -r '.funding[].symbol' | sort)
```

No output usually means the discovered contract universe and funding rows are
aligned for that exchange and quote filter.

## 13. Generate Watchlist Files

Write Binance extreme negative funding symbols to a plain text file:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[].symbol' \
> watchlist_binance_negative_funding.txt
```

Save a multi-exchange JSON watchlist:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '{
    generated_from: "marketbridge",
    filter: {min_pct: -2, max_pct: -0.2, quote: "USDT"},
    rows: (.funding
      | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
      | sort_by(.funding_rate_pct))
  }' \
> watchlist_negative_funding.json
```

## 14. Troubleshooting

Check whether the service is running:

```bash
curl -s "$MB/health" | jq
```

Check exchange-adapter errors:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.errors'
```

Check returned row count:

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding | length'
```

If `errors[]` is non-empty, treat the response as partial. Do not interpret an
empty filtered result as "no matching contracts" until you have checked whether
the exchange adapter failed or was rate-limited.
