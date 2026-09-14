# 加密研究的宏观上下文

> **研究问题：** 公开宏观参考、ETF 流量或聚合状态标签，在时间对齐后是否能区分后续加密市场响应？

## 案例

- `crypto_macro_context_monitor.py` / recorder / replay：把配置的 DXY、VIX、US10Y、永续资金费率
  与 BTC 报价配对。
- `crypto_etf_flow_response_recorder.py` / replay：把调用者提供或 `farside_etf` 的流量快照对齐到 BTC 日 K 线。
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

## 解释与边界

宏观上下文只能作为条件信息，不是收益预测或资金分配决定。示例不交易 ETF、不再平衡组合，
也不把相关性解释为因果。每份报告都要保留日历对齐、发布时间延迟、缺失观测、样本量和纸面成本。

CSV 格式、完整命令和出处见 [`README.md`](README.md)。
