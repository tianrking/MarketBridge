# Macro context for crypto research / 加密研究的宏观上下文

> **Language / 语言**: [English](README.en.md) · [简体中文](README.zh-CN.md)
>
> For the polished quickstart and boundary notes, start with the language-specific guide.

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

`crypto_etf_flow_response_recorder.py` / `crypto_etf_flow_response_replay.py`
form the explicit external-data bridge for
spot ETF flow research. It reads a Farside-style CSV in USD millions, aligns
each trading date with MarketBridge daily BTC candles, and compares large
inflow, large outflow and ordinary-flow buckets at a fixed forward horizon.
The CSV is intentionally caller-supplied because ETF flows are not yet a
native MarketBridge historical endpoint; the output names that boundary rather
than silently treating missing flow data as zero.
When `aggregates.farside_etf.enabled: true` (default URL:
`https://farside.co.uk/bitcoin-etf-flow-all-data/`), the recorder can use the latest
MarketBridge `/v1/external/signals?sources=farside_etf` snapshot directly and
freeze it beside `/v1/market/quotes`; replaying the resulting JSONL avoids a
manual CSV step.
If the public page returns a bot-protection challenge, the connector logs the
fetch failure and emits no signal; use the documented CSV fallback or a
credential-free mirror rather than treating the challenge page as data.

Provenance: VIX semantics are cross-checked against [Cboe's VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs),
which describes the index as derived from SPX option inputs; dollar-index context
is cross-checked against the [Federal Reserve H.10 dollar-index documentation](https://www.federalreserve.gov/releases/h10/Summary/).
These are reference-data definitions, not a claim that macro snapshots predict
crypto returns.
The risk-context hypothesis is also motivated by [Wintermute's public macro and
crypto-liquidity discussion on X](https://x.com/wintermute_t/status/1985631560021000352),
which is treated as an unverified research lead rather than a forecast.
The ETF-flow lead is cross-checked against [Farside's daily Bitcoin ETF flow
table](https://farside.co.uk/btc/). It is a measurement source, not evidence
that flows cause price movement.

## 中文

这一系列把已配置的 DXY、VIX、US10Y 参考报价与当前 crypto 永续资金费率放在同一个只读上下文中。
它只标注研究背景：`elevated_volatility_context`、`normal_volatility_context` 或明确的数据缺失状态，
不预测加密资产收益、不选择策略，也不执行交易。

`crypto_macro_context_recorder.py` / `crypto_macro_context_replay.py` 增加时间生命周期：冻结同一组 DXY/VIX/US10Y
参考报价、资金费率状态和 BTC 价格，再按宏观波动与资金拥挤分桶报告未来收益、绝对波动和下行比例。
它是响应分布研究，不是宏观方向信号；参考报价的时间戳来自提供方快照，不是同步的历史指数序列。

`crypto_etf_flow_response_recorder.py` / `crypto_etf_flow_response_replay.py` 是外部数据的明确接入桥：读取 USD 百万单位的 Farside 风格 CSV，
把每个交易日和 MarketBridge 的 BTC 日线对齐，再比较大额流入、大额流出与普通流量在固定窗口后的响应。
由于 ETF 流量还不是 MarketBridge 原生历史接口，CSV 必须由调用者提供；输出会明确这一边界，不会把缺失流量填成零。
启用 `aggregates.farside_etf.enabled: true`（默认 URL 为
`https://farside.co.uk/bitcoin-etf-flow-all-data/`）后，recorder 也可以直接读取
MarketBridge `/v1/external/signals?sources=farside_etf` 的最新快照，并和 `/v1/market/quotes` 一起冻结；
回放 JSONL 时不需要手工 CSV。
如果公开页面返回 bot-protection challenge，connector 会记录抓取失败且不发出信号；应使用文档中的 CSV
回退或无凭证镜像，不要把 challenge 页面当作数据。

出处：VIX 语义对照 [Cboe VIX FAQ](https://www.cboe.com/tradable_products/vix/faqs)，其中说明该指数来自
SPX 期权输入；美元指数上下文对照 [Federal Reserve H.10 美元指数文档](https://www.federalreserve.gov/releases/h10/Summary/)。
这些资料只是参考数据定义，不代表宏观快照能预测 crypto 收益。
风险上下文假设也参考 [Wintermute 在 X 的宏观与 crypto 流动性讨论](https://x.com/wintermute_t/status/1985631560021000352)，
但该内容只作为未经验证的研究线索，不作为预测。
ETF 流量出处对照 [Farside 的 Bitcoin ETF 日流量表](https://farside.co.uk/btc/)，它是测量来源，不表示流量能够造成价格变动。

## Commands / 命令

The CSV needs a date column and an aggregate total-flow column in USD
millions; parenthesized values are treated as outflows. For example:

```csv
Date,Total
2026-09-01,125.5
2026-09-02,(80.0)
```

CSV 需要日期列和 USD 百万单位的 aggregate total-flow 列；括号值会被解析为流出。例如：

```csv
Date,Total
2026-09-01,125.5
2026-09-02,(80.0)
```

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
python3 examples/crypto/macro/crypto_etf_flow_response_replay.py \
  --input work/btc-etf-flows.csv --exchange binance --symbol BTCUSDT \
  --interval 1d --threshold-musd 100 --horizon-days 1 \
  --min-observations 5 --paper-cost-bps 10
python3 examples/crypto/macro/crypto_etf_flow_response_recorder.py \
  --asset BTC --symbol BTCUSDT --exchange binance --iterations 10 \
  --interval-secs 900 --output work/crypto-etf-flow-response.jsonl
python3 examples/crypto/macro/crypto_etf_flow_response_replay.py \
  --input work/crypto-etf-flow-response.jsonl --exchange binance \
  --symbol BTCUSDT --interval 1d --threshold-musd 100 --horizon-days 1
```
