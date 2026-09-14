# 加密资金费率与套利研究

> **研究问题：** basis、资金费率或跨交易所关系出现异常时，这个可观察状态是否会持续，
> 或在固定窗口后呈现可测量的响应？

## 本目录内容

| 分组 | 入口 | 输出证据 |
|---|---|---|
| Basis 与资金费率 | `basis_carry_monitor.py`、`funding_convergence_monitor.py`、`crypto_funding_band_monitor.py` | 新鲜 spot/perp basis、资金费率、周期、提供方上下限和缺失字段 |
| 历史回放 | `crypto_basis_replay.py`、`crypto_historical_basis_replay.py`、`crypto_funding_*_replay.py` | 固定窗口收敛、回归或状态分布 |
| 跨交易所 | `crypto_cross_venue_orderbook_*`、`crypto_cross_venue_price_gap_replay.py` | 带延迟和覆盖元数据的逐时点报价/盘口差 |
| Coinbase premium | `crypto_coinbase_premium_monitor.py`、`crypto_coinbase_premium_response_recorder.py`、`crypto_coinbase_premium_response_replay.py` | Coinbase USD 相对参考 venue 的现货价差和 BTC 后续响应 |
| 三角价差 | `crypto_triangular_arbitrage_*` | 纸面价格循环一致性；不路由、不成交 |
| 响应研究 | `crypto_*_response_recorder.py` / `*_response_replay.py` | 冻结状态与 BTC 报价，再比较后续收益 |

资金费率上下限响应 pair 是最新案例：把提供方资金费率分类为
`near_upper_funding_cap`、`near_lower_funding_floor` 或
`within_provider_funding_band`，然后比较后续 BTC movement。它不把上下限附近观察
变成资金费率收益、对冲或交易。

Coinbase premium pair 是独立的公开 X 研究线索：当 Coinbase USD 报价相对参考现货明显升水或折价时，
下一固定记录窗口的 BTC 响应是否不同于普通快照？默认参考是 Binance `BTCUSDT`；USD/USDT 稳定币基差会作为限制保留，
不会被静默称作美国现货流量。
`crypto_coinbase_premium_historical_replay.py` 还可以直接使用 `/v1/history/candles` 提供的 Coinbase 和参考 venue candles，
在有界历史上复核同一假设，不依赖实时 recorder 才能产生证据。

历史 OI 现在支持 Binance、Bybit 和 OKX。OKX 公共合约历史接口返回按基础币种聚合、以提供方 USD 单位计量的序列；
API 会保留单位和时间戳。这样 `crypto_funding_oi_replay.py` 可以扩大交易所比较范围，但不会把聚合 OI 解释成多空归属或对冲结果。

`crypto_funding_carry_accrual_replay.py` 是状态回放之外的纸面账本：每个固定 funding 事件窗口分别计算每单位名义本金的资金费率转移、
观察到的现货/永续基差变化，以及 `short_perp` 或 `long_perp` 方向下的带符号纸面合计。它不是可执行 carry 回测：公共收盘价不是成交价，
借币、保证金、抵押品、手续费和滑点都不在账本内。

`crypto_historical_basis_replay.py` 是独立的提供方基差假设：当 Binance basis-rate 明显偏宽时，
接下来固定的提供方窗口里，绝对基差是否比普通状态更容易收敛？当单个 500 行页面不够覆盖研究区间时，
使用 `--basis-pages`（1–48）。API 会保留请求页数和实际覆盖时间戳；Binance 公共历史仍有保留上限，
所以这不是完整历史，也不是可执行 carry PnL。

```bash
python3 examples/crypto/carry/crypto_historical_basis_replay.py \
  --symbol BTCUSDT --period 1h --days 20 --limit 500 --basis-pages 2 \
  --horizon-bars 3 --extreme-threshold-bps 50 --min-observations 5
```

```bash
python3 examples/crypto/carry/crypto_funding_carry_accrual_replay.py \
  --symbol BTCUSDT --funding-exchange binance \
  --spot-exchange binance --perp-exchange binance \
  --price-interval 5m --days 7 --horizon-events 3 \
  --position-side short_perp --min-observations 5
```

## 完整运行示例

```bash
python3 examples/crypto/carry/crypto_funding_band_monitor.py \
  --symbol BTCUSDT --exchange binance --threshold 0.8
python3 examples/crypto/carry/crypto_funding_band_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 600 \
  --output work/crypto-funding-band-response.jsonl
python3 examples/crypto/carry/crypto_funding_band_response_replay.py \
  --input work/crypto-funding-band-response.jsonl \
  --horizon-records 3 --min-observations 5
```

维护中的命令见本中文指南。recorder 采用追加写入；开始
独立样本前应有意清理旧 JSONL，避免不同研究混在一起。

## 解释规则

- 缺少资金周期或提供方上下限时，结果表示元数据缺失，不能填成零或推导年化收益。
- `horizon-records` 是记录条数，不自动等于小时数；比较研究前先检查时间戳。
- Basis 和跨交易所价差在没有借贷、手续费、转账延迟、库存、滑点、排队和成交覆盖模型时，
  只能称为 gross observation。
- 每个 recorder/replay 结果都带有 `research_only_no_orders`。

```bash
python3 examples/crypto/carry/crypto_coinbase_premium_monitor.py \
  --coinbase-symbol BTC-USD --reference-symbol BTCUSDT \
  --reference-exchange binance --premium-threshold-bps 5
python3 examples/crypto/carry/crypto_coinbase_premium_response_recorder.py \
  --coinbase-symbol BTC-USD --reference-symbol BTCUSDT \
  --reference-exchange binance --iterations 60 --interval-secs 60 \
  --output work/crypto-coinbase-premium-response.jsonl
python3 examples/crypto/carry/crypto_coinbase_premium_response_replay.py \
  --input work/crypto-coinbase-premium-response.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/carry/crypto_coinbase_premium_historical_replay.py \
  --coinbase-symbol BTCUSDT --reference-symbol BTCUSDT \
  --reference-exchange binance --interval 1h --days 14 \
  --horizon-bars 3 --min-observations 5
```

## 出处

出处在原 README 和 development log 中集中维护。提供方上下限语义以 Binance
[Funding Rate Info API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Get-Funding-Info)
为准；公开 X 仅是待验证线索。当前上下限研究线索来自[公开资金费率讨论](https://x.com/instaclaws/status/2038363051213181035)。
Coinbase premium 线索来自 [XWIN flow-confirmation 讨论](https://x.com/xwinfinance/status/2023155692916646257)，字段语义对照
官方 [Coinbase Exchange candles API](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles)。
纸面 carry 账本的现金流分解参考 Binance 的[资金费率套利说明](https://www.binance.com/en/support/faq/detail/61012e690cf343e7979649282a2ccc3c)
和 Kraken 的[资金费率策略说明](https://www.kraken.com/learn/futures-trading-funding-rate-strategy)，只用于定义研究变量，不表示收益保证。

## 边界

本目录只观察和回放公开市场数据，不建立对冲、不借币、不转账、不签名钱包、不发送订单。
