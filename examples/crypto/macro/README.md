# Macro context for crypto research / 加密研究的宏观上下文

## English

This family exposes configured DXY, VIX and US10Y reference quotes alongside a
current crypto perpetual funding row. It labels only a research backdrop:
`elevated_volatility_context`, `normal_volatility_context` or an explicit
missing-data state. It does not forecast crypto returns, select a strategy or
execute trades.

`crypto_macro_context_recorder.py` / `crypto_macro_context_replay.py` add a
temporal research lifecycle. They freeze the same DXY/VIX/US10Y reference
quotes, funding state and BTC price, then report forward-return, absolute-move
and downside distributions by macro-volatility and funding-crowding bucket.
The result is a response study, not a directional macro signal: reference
timestamps are provider snapshots and not a synchronized historical index
series.

Provenance: VIX semantics are cross-checked against [Cboe's VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs),
which describes the index as derived from SPX option inputs; dollar-index context
is cross-checked against the [Federal Reserve H.10 dollar-index documentation](https://www.federalreserve.gov/releases/h10/Summary/).
These are reference-data definitions, not a claim that macro snapshots predict
crypto returns.
The risk-context hypothesis is also motivated by [Wintermute's public macro and
crypto-liquidity discussion on X](https://x.com/wintermute_t/status/1985631560021000352),
which is treated as an unverified research lead rather than a forecast.

## 中文

这一系列把已配置的 DXY、VIX、US10Y 参考报价与当前 crypto 永续资金费率放在同一个只读上下文中。
它只标注研究背景：`elevated_volatility_context`、`normal_volatility_context` 或明确的数据缺失状态，
不预测加密资产收益、不选择策略，也不执行交易。

`crypto_macro_context_recorder.py` / `crypto_macro_context_replay.py` 增加时间生命周期：冻结同一组 DXY/VIX/US10Y
参考报价、资金费率状态和 BTC 价格，再按宏观波动与资金拥挤分桶报告未来收益、绝对波动和下行比例。
它是响应分布研究，不是宏观方向信号；参考报价的时间戳来自提供方快照，不是同步的历史指数序列。

出处：VIX 语义对照 [Cboe VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs)，其中说明该指数来自
SPX 期权输入；美元指数上下文对照 [Federal Reserve H.10 美元指数文档](https://www.federalreserve.gov/releases/h10/Summary/)。
这些资料只是参考数据定义，不代表宏观快照能预测 crypto 收益。
风险上下文假设也参考 [Wintermute 在 X 的宏观与 crypto 流动性讨论](https://x.com/wintermute_t/status/1985631560021000352)，
但该内容只作为未经验证的研究线索，不作为预测。

## Commands / 命令

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
