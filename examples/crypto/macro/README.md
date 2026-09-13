# Macro context for crypto research / 加密研究的宏观上下文

## English

This family exposes configured DXY, VIX and US10Y reference quotes alongside a
current crypto perpetual funding row. It labels only a research backdrop:
`elevated_volatility_context`, `normal_volatility_context` or an explicit
missing-data state. It does not forecast crypto returns, select a strategy or
execute trades.

Provenance: VIX semantics are cross-checked against [Cboe's VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs),
which describes the index as derived from SPX option inputs; dollar-index context
is cross-checked against the [Federal Reserve H.10 dollar-index documentation](https://www.federalreserve.gov/releases/h10/Summary/).
These are reference-data definitions, not a claim that macro snapshots predict
crypto returns.

## 中文

这一系列把已配置的 DXY、VIX、US10Y 参考报价与当前 crypto 永续资金费率放在同一个只读上下文中。
它只标注研究背景：`elevated_volatility_context`、`normal_volatility_context` 或明确的数据缺失状态，
不预测加密资产收益、不选择策略，也不执行交易。

出处：VIX 语义对照 [Cboe VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs)，其中说明该指数来自
SPX 期权输入；美元指数上下文对照 [Federal Reserve H.10 美元指数文档](https://www.federalreserve.gov/releases/h10/Summary/)。
这些资料只是参考数据定义，不代表宏观快照能预测 crypto 收益。

## Commands / 命令

```bash
python3 examples/crypto/macro/crypto_macro_context_monitor.py \
  --symbol BTCUSDT --exchange binance --vix-risk-threshold 25 \
  --funding-extreme-pct 0.01
```
