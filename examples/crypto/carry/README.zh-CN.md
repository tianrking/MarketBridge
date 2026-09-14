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

所有命令见本目录原始双语目录 [`README.md`](README.md)。recorder 采用追加写入；开始
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
```

## 出处

出处在原 README 和 development log 中集中维护。提供方上下限语义以 Binance
[Funding Rate Info API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Get-Funding-Info)
为准；公开 X 仅是待验证线索。当前上下限研究线索来自[公开资金费率讨论](https://x.com/instaclaws/status/2038363051213181035)。
Coinbase premium 线索来自 [XWIN flow-confirmation 讨论](https://x.com/xwinfinance/status/2023155692916646257)，字段语义对照
官方 [Coinbase Exchange candles API](https://docs.cdp.coinbase.com/api-reference/exchange-api/rest-api/products/get-product-candles)。

## 边界

本目录只观察和回放公开市场数据，不建立对冲、不借币、不转账、不签名钱包、不发送订单。
