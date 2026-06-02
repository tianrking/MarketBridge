# 指定币种逼空与做空机会监控示例

本文说明如何用 MarketBridge 当前数据面分析指定币种状态，并运行两个只读示例：

- `/v1/research/symbol-state`：服务内实时状态机，持续消费事件流并输出当前阶段。
- `short_squeeze_monitor`：跟随做多，捕捉逼空 / Short Squeeze 共振。
- `exhaustion_short_monitor`：寻找做空机会，捕捉流动性枯竭 / 派发结束。

MarketBridge 的边界仍然是数据统一层：它不下单、不签名、不证明因子有效。状态机和示例只输出只读信号、指标、阶段和风控上下文。

## 当前项目能提供的数据

| 监控维度 | 当前能力 | 主要接口 | 说明 |
|---|---|---|---|
| 价格 / spot-perp basis | 已实现 | `/v1/market/quotes`, `/v1/market/basis`, `/v1/market/klines` | 可看价格、基差、K 线走势。 |
| Tick / book / trade / OI 长期存储 | 已实现，可选启用 | `runtime.clickhouse` | 通过 ClickHouse HTTP JSONEachRow 批量写入多张 MergeTree 表。 |
| Funding Rate | 已实现 | `/v1/market/funding` | 多数 perp venue keyless；CoinGlass 聚合需要 API key。 |
| Open Interest | 已实现 | `/v1/market/open-interest` | 接口给最新 OI；示例通过持续轮询计算 OI 变化率。 |
| CVD / 主动买卖 | 已实现 | `/v1/market/order-flow`, `/v1/market/footprint` | 由实时 trade 事件计算 buy/sell delta、CVD、footprint。 |
| L2 订单簿厚度 / OFI | 已实现 | `/v1/market/order-books`, `/v1/research/symbol-state` | 状态机实时计算前 10 档深度比例、盘口压力和 best-level OFI。 |
| 清算流 | 部分实现 | `/v1/market/liquidations` | 取决于交易所是否有稳定公开清算 feed。 |
| CoinGlass 聚合衍生品指标 | 已实现但需 key | `/v1/external/signals?sources=coinglass` | funding/OI/liquidation/long-short/basis/options OI 聚合指标。 |
| 链上大额转账 | 已实现但需配置 | `/v1/onchain/transfers` | Whale Alert/Etherscan 需要 API key；Etherscan 是地址 watchlist，不是全链 firehose。 |
| 清算热力图价格墙 | 当前缺口 | 无原生接口 | 需要新增 heatmap 数据源，或用 `aggregates.custom_apis` 接第三方 JSON。 |
| 巨鲸 / 项目方转入 CEX 精确识别 | 部分缺口 | `/v1/onchain/transfers` | 需要交易所地址标签、项目方/巨鲸地址 watchlist、或 Whale Alert 标签质量。 |

## 实时状态机 API

启动 MarketBridge 后，查询指定币种：

```powershell
curl -s "http://127.0.0.1:8080/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance"
```

返回的核心字段：

| 字段 | 含义 |
|---|---|
| `metrics.funding_rate` | 当前资金费率。 |
| `metrics.open_interest_change_pct` | 服务运行期间相邻 OI 快照变化率。 |
| `metrics.spot_cvd_notional_1m` | 现货最近 1 分钟 CVD 名义金额。 |
| `metrics.perp_cvd_notional_1m` | 合约最近 1 分钟 CVD 名义金额。 |
| `metrics.cvd_divergence` | CVD 背离标签，例如 `spot_up_perp_down`。 |
| `metrics.bid_ask_depth_ratio_10` | 合约盘口前 10 档 bid / ask 深度比。 |
| `metrics.depth_pressure_10` | `(bid_depth - ask_depth) / total_depth`。 |
| `metrics.ofi_best_level_1m` | 最近 1 分钟 best-level OFI 名义金额。 |
| `metrics.buy_liquidation_notional_15m` | 最近 15 分钟买向清算名义金额。 |
| `long_squeeze.state` | 逼空做多阶段。 |
| `short_exhaustion.state` | 枯竭做空阶段。 |
| `risk_context` | 只读风控上下文；不执行下单。 |

逼空状态阶段：

```text
neutral -> short_crowding -> chip_accumulation -> spot_absorption -> triggered_long_squeeze
```

做空枯竭状态阶段：

```text
neutral -> long_crowding -> fuel_exhaustion -> book_vacuum -> triggered_short_exhaustion
```

注意：OI 变化、OFI、CVD 都需要服务先运行并接收到至少两次相关事件。刚启动时字段为 `null` 是正常的。

## 示例一：逼空跟随做多

核心共振：

1. Funding 持续深度负值，例如 `< -0.05%`。
2. OI 在价格横盘或缓跌时继续上升。
3. Spot CVD 向上，perp CVD 向下。
4. 上方存在清算目标，或至少观察到清算流/聚合清算指标。

运行：

```powershell
cargo run --example short_squeeze_monitor -- --symbol BTCUSDT --exchange binance --iterations 5 --interval-secs 30
```

输出会给出 `score` 和每条证据。OI 变化率必须至少轮询两次后才有，因为当前 REST 快照不是历史序列。

当前可直接判断：

- funding 是否极端负值；
- OI 是否在服务运行期间扩张；
- spot/perp CVD 是否背离；
- native liquidation feed 是否出现近期清算；
- CoinGlass 聚合 liquidation 是否可用。

当前不能完整判断：

- 50x/100x 清算墙具体价格位置；
- “主力知道拉到哪个价格会连环爆仓”的热力图结构。

要补齐这一项，需要接入 liquidation heatmap provider，并把数据标准化成 `external_signal` 或新增结构化 domain，例如 `liquidation_heatmap`，字段至少包含 `symbol`、`side`、`price_level`、`leverage_band`、`estimated_notional`、`ts_ms`。

## 示例二：流动性枯竭 / 派发结束做空

核心共振：

1. Funding 很高，但价格冲高失败。
2. 价格创新高后 OI 下降，说明上涨主要来自空头回补，燃料减少。
3. Perp CVD 转弱或卖方主动成交增强。
4. 买盘墙薄、卖盘墙厚，下方出现真空区。
5. 巨鲸或项目方大额转入 CEX。

运行：

```powershell
cargo run --example exhaustion_short_monitor -- --symbol BTCUSDT --exchange binance --iterations 5 --interval-secs 30
```

当前可直接判断：

- funding 是否拥挤偏多；
- OI 是否在两次轮询之间下降；
- 近 1m K 线是否出现冲高回落；
- perp CVD 是否转弱；
- L2 前 10 档 bid/ask 深度比例是否偏空。

当前只能部分判断：

- 巨鲸/项目方转入 CEX。MarketBridge 有 on-chain transfer store，但需要启用 Whale Alert 或配置 Etherscan watchlist，并依赖地址标签。

建议配置：

```yaml
onchain:
  whale_alert:
    enabled: true
    api_key_env: WHALE_ALERT_API_KEY
    min_value_usd: 1000000
  etherscan:
    enabled: true
    api_key_env: ETHERSCAN_API_KEY
    min_value_eth: 1000
    addresses:
      - "0x..."
```

## 指定币种分析路线

给定 `BTCUSDT`、`ETHUSDT` 或其他合约符号，推荐调用顺序：

1. `/v1/agent/context?symbols=BTCUSDT&include_storage=true`
2. `/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance`
3. `/v1/research/features?symbols=BTCUSDT&exchange=binance&market=perp&intervals=1m,5m,15m`
4. `/v1/market/funding?symbols=BTCUSDT`
5. `/v1/market/open-interest?symbols=BTCUSDT`
6. `/v1/market/order-flow?symbol=BTCUSDT&market=spot&window_ms=60000`
7. `/v1/market/order-flow?symbol=BTCUSDT&market=perp&window_ms=60000`
8. `/v1/market/order-books?symbols=BTCUSDT&market=perp`
9. `/v1/market/liquidations?symbols=BTCUSDT`
10. `/v1/onchain/transfers` 和 `/v1/external/signals?sources=coinglass`

如果要变成生产级实盘系统，还需要补齐两块：清算热力图价格墙结构化数据源、独立执行与风控服务。MarketBridge 当前内置状态机是只读实时分析层，不会突破“不下单、不签名”的系统边界。

## ClickHouse 存储

Tick、订单簿、逐笔成交、funding、OI、清算和外部信号可以写入 ClickHouse。先启动 ClickHouse：

```powershell
docker run --rm -p 8123:8123 -p 9000:9000 --name marketbridge-clickhouse clickhouse/clickhouse-server:latest
```

然后在配置中启用：

```yaml
runtime:
  clickhouse:
    enabled: true
    url: "http://127.0.0.1:8123"
    database: "marketbridge"
    batch_max: 1000
    flush_ms: 250
    init_tables: true
```

MarketBridge 会创建：

- `marketbridge.market_quotes`
- `marketbridge.trades`
- `marketbridge.order_books`
- `marketbridge.funding_rates`
- `marketbridge.open_interest`
- `marketbridge.liquidations`
- `marketbridge.external_signals`
