# 加密研究的宏观上下文

> **研究问题：** 公开宏观参考、ETF 流量或聚合状态标签，在时间对齐后是否能区分后续加密市场响应？

## 案例

- `crypto_macro_context_monitor.py` / recorder / replay：把配置的 DXY、VIX、US10Y、永续资金费率
  与 BTC 报价配对。
- `crypto_etf_flow_response_recorder.py` / replay：把调用者提供或 `farside_etf` 的流量快照对齐到 BTC 日 K 线；回放还支持滚动流量窗口，
  用来把持续性与单日阈值分开检验。
- `crypto_liquidity_confirmation_monitor.py` / recorder / replay：把 ETF 流量、稳定币供应、Coinbase 溢价和资金拥挤保持为独立通道，
  再评估透明的确认矩阵。
- `crypto_liquidity_impulse_replay.py`：把滚动 Farside ETF 流量窗口与精确日期的
  DefiLlama 稳定币供应变化连接起来，再测量后续 BTC K 线响应的历史双通道研究。
  回放支持有界 Binance K 线分页，避免多年日线样本静默停在单个 1,500 行页面。
- `crypto_market_regime_monitor.py` / recorder / replay：Rust 聚合 regime 标签和固定窗口 BTC 响应分布。

## 快速开始

```bash
python3 examples/crypto/macro/crypto_macro_context_monitor.py \
  --symbol BTCUSDT --exchange binance --vix-risk-threshold 25 \
  --funding-extreme-pct 0.01
python3 examples/crypto/macro/crypto_macro_context_recorder.py \
  --symbol BTCUSDT --exchange binance --product-type perp \
  --iterations 30 --interval-secs 30 \
  --output work/crypto-macro-context.jsonl
python3 examples/crypto/macro/crypto_macro_context_replay.py \
  --input work/crypto-macro-context.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 10
```

ETF 流量被明确保留为外部数据边界：可以使用 Farside 风格 CSV 或已配置 connector，但页面
被拦截时是抓取失败，不是零流量。提供方时间戳是快照，不能冒充同步的历史指数序列。

要做滚动窗口研究，可传 `--rolling-observations 5`。回放只使用当前行和此前的外部流量行，
窗口不完整时跳过，并分类为 `rolling_inflow`、`rolling_outflow` 或 `rolling_neutral`。
默认滚动阈值等于“单日阈值 × 窗口长度”；如果研究问题需要累计 USD 百万阈值，可用
`--rolling-threshold-musd` 明确覆盖。

持续流量线索参考公开的 [ecoinometrics ETF 流量讨论](https://x.com/ecoinometrics/status/2037548621697303004)，
仅作为未经验证的研究假设，不代表滚动阈值可以预测 BTC。

新的 impulse 回放把公开流动性叙事拆成更窄的可证伪问题：ETF 流入/流出与稳定币供应扩张/收缩
同时出现时，后续固定 BTC 窗口是否不同？它读取调用者提供的 Farside 风格 CSV 或 JSONL，使用
`/v1/history/stablecoins` 的 UTC 日期和 MarketBridge BTC K 线；日期缺失会跳过，稳定币供应不被当作
交易所库存或已确认的买盘。

四通道确认线索参考 [XWIN 的趋势与流量确认讨论](https://x.com/xwinfinance/status/2023155692916646257)
以及 [Wintermute 的流动性通道讨论](https://x.com/wintermute_t/status/1985631560021000352)。
监控器要求至少 `--min-confirmations` 个正向或负向通道可观测，但输出仍只是上下文标签，不是价格预测；资金费率只作为拥挤诊断，
不会计入流动性分数。

```bash
python3 examples/crypto/macro/crypto_liquidity_confirmation_monitor.py \
  --symbol BTCUSDT --exchange binance --min-confirmations 2
python3 examples/crypto/macro/crypto_liquidity_confirmation_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 900 \
  --output work/crypto-liquidity-confirmation.jsonl
python3 examples/crypto/macro/crypto_liquidity_confirmation_replay.py \
  --input work/crypto-liquidity-confirmation.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/macro/crypto_liquidity_impulse_replay.py \
  --etf-flow-csv work/btc-etf-flows.csv --chain all \
  --exchange binance --symbol BTCUSDT --interval 1d --candle-pages 2 \
  --flow-window-observations 5 --supply-change-window-days 7 \
  --etf-threshold-musd 100 --supply-threshold-pct 1 \
  --horizon-days 7 --min-observations 5
```

## 解释与边界

宏观上下文只能作为条件信息，不是收益预测或资金分配决定。示例不交易 ETF、不再平衡组合，
也不把相关性解释为因果。每份报告都要保留日历对齐、发布时间延迟、缺失观测、样本量和纸面成本。

CSV 格式、维护中的命令和出处见本中文指南。
