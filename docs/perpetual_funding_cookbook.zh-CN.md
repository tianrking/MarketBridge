# 永续合约与资金费率常用查询脚本

这些例子都把 MarketBridge 当作统一数据接口使用。MarketBridge 返回原始归一化数据，`jq` 或 Python 脚本负责筛选、排序、导出和告警。

先启动 MarketBridge：

```bash
cd /home/w0x7ce/Downloads/dm_candrive/MarketBridge
MARKETBRIDGE_CONFIG=./config.min.yaml cargo run
```

另一个终端设置一个基础变量：

```bash
MB="http://127.0.0.1:8080"
curl -s "$MB/health" | jq
```

如果启用了 API key：

```bash
export MARKETBRIDGE_API_KEY="your-key"
curl -H "x-api-key: $MARKETBRIDGE_API_KEY" -s "$MB/health" | jq
```

下面的例子默认没有启用 API key。`funding_rate_pct` 已经是百分比单位，所以 `-0.2` 表示 `-0.2%`，`1.5` 表示 `1.5%`。

## 1. 查询某交易所有哪些永续合约

Binance USDT 永续合约，前 20 个：

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=20" \
| jq '.exchanges[0].contracts[]
  | {exchange, symbol, native_symbol, base, quote, active, status, contract_type}'
```

只看合约数量和 base 数量：

```bash
curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts_total, contracts_returned, base_assets_total}'
```

多个交易所各自有多少 USDT 永续：

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts_total, base_assets_total}'
```

查某个 base，例如 BTC，在不同交易所的永续合约：

```bash
curl -s "$MB/v1/catalog/perpetuals?exchanges=binance,okx,bybit,bitget&base=BTC&limit=50000" \
| jq '.exchanges[]
  | {exchange, contracts: [.contracts[] | {symbol, native_symbol, quote, settle_asset}]}'
```

## 2. 查询某交易所全部当前资金费率

Binance 当前 USDT 永续资金费率，返回简洁字段：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding[]
  | {exchange, symbol, funding_rate_pct, mark_price, index_price, next_funding_time_ms}'
```

查看一批交易所是否有 adapter 错误：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '{rows:(.funding | length), errors}'
```

按交易所统计资金费率行数、最低、最高：

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

## 3. 找 Binance 资金费率在 -2% 到 -0.2% 的永续

这是常见的极端负费率搜索：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

只输出 symbol，适合做 watchlist：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[].symbol'
```

用 Python 示例脚本做同样的事情：

```bash
./examples/funding_extremes.py --exchange binance --quote USDT --min-pct -2 --max-pct -0.2
```

JSON 输出，方便给别的程序消费：

```bash
./examples/funding_extremes.py --exchange binance --quote USDT --min-pct -2 --max-pct -0.2 --json | jq
```

## 4. 找多个交易所的极端负费率

查 Binance、OKX、Bybit、Bitget，区间 `-2%` 到 `-0.2%`：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

按交易所分组显示：

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

## 5. 找最极端的负费率 Top 20

不限定下限，直接找最低的 20 个：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | sort_by(.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

只看小于等于 `-0.1%` 的 Top 20：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct <= -0.1))
  | sort_by(.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 6. 找极端正费率

正费率过高通常代表多头拥挤。下面找 `0.2%` 到 `2%`：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= 0.2 and .funding_rate_pct <= 2))
  | sort_by(-.funding_rate_pct)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price, next_funding_time_ms}'
```

找正费率最高 Top 20：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&quote=USDT&limit=50000" \
| jq '.funding
  | sort_by(-.funding_rate_pct)
  | .[:20]
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 7. 找接近中性的资金费率

例如 `-0.005%` 到 `0.005%`：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding
  | map(select(.funding_rate_pct >= -0.005 and .funding_rate_pct <= 0.005))
  | sort_by(.symbol)
  | .[]
  | {exchange, symbol, funding_rate_pct, mark_price}'
```

## 8. 查某个 symbol 在多个交易所的资金费率

BTCUSDT 和 ETHUSDT 跨交易所对比：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=BTCUSDT,ETHUSDT&limit=50000" \
| jq '.funding
  | sort_by(.symbol, .exchange)
  | .[]
  | {symbol, exchange, funding_rate_pct, mark_price, next_funding_time_ms}'
```

某个山寨币，例如 AERGOUSDT，在哪些交易所有资金费率：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchanges=binance,okx,bybit,bitget&symbols=AERGOUSDT&limit=50000" \
| jq '.funding
  | sort_by(.funding_rate_pct)
  | .[]
  | {symbol, exchange, funding_rate_pct, mark_price, source}'
```

## 9. 计算同一 symbol 的跨交易所资金费率差

例如找多交易所都有的 symbol，并计算最高和最低资金费率差：

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

只看资金费率差大于 `0.2%` 的 symbol：

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

## 10. 导出 CSV

导出 Binance `-2%` 到 `-0.2%` 的结果：

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

导出多个交易所正负极端资金费率：

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

## 11. 把时间戳转成人类可读时间

`next_funding_time_ms` 是 Unix 毫秒，可以用 `jq` 转 UTC 时间：

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

## 12. 找合约存在但资金费率接口没返回的 symbol

这可以帮助判断某交易所的 funding adapter 是否有覆盖缺口。下面以 Binance 为例：

```bash
comm -23 \
  <(curl -s "$MB/v1/catalog/perpetuals?exchange=binance&quote=USDT&limit=50000" \
    | jq -r '.exchanges[0].contracts[].symbol' | sort) \
  <(curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
    | jq -r '.funding[].symbol' | sort)
```

如果没有输出，说明 discovery 和 funding 返回的 symbol 基本对齐。

## 13. 生成监控 watchlist 文件

把 Binance 极端负费率 symbol 写入文件：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq -r '.funding
  | map(select(.funding_rate_pct >= -2 and .funding_rate_pct <= -0.2))
  | sort_by(.funding_rate_pct)
  | .[].symbol' \
> watchlist_binance_negative_funding.txt
```

把多交易所结果保存为 JSON：

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

## 14. 常见排错

确认服务是否启动：

```bash
curl -s "$MB/health" | jq
```

确认接口是否有交易所错误：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.errors'
```

确认返回行数：

```bash
curl -s "$MB/v1/market/perpetual-funding?exchange=binance&quote=USDT&limit=50000" \
| jq '.funding | length'
```

如果你看到 `errors` 非空，不要把空结果直接理解为“没有符合条件的合约”。先看 `errors[]`，确认是否某个交易所请求失败或限流。
