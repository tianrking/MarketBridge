# Options and volatility / 期权与波动率

## English

This family separates observable surface features from unobservable position
ownership. Skew and term structure use transparent moneyness buckets; VRP
compares option ATM IV with perp realized volatility; gamma maps use unsigned
`gamma × OI × underlying²` mass. Deribit summary rows may omit greeks, so the
gamma monitor uses bounded `/options/deribit/book` enrichment and reports
fetched/unfetched coverage. No example infers dealer gamma sign, option PnL,
hedge ratios or execution.

The VRP recorder/replay freezes the IV-minus-RV snapshot before testing whether
an implied-volatility-premium state persists for one expiry. It is a regime
diagnostic, not a short-volatility recommendation: maturities, RV windows,
delta hedging, fills and costs are not matched.
The VRP monitor also preserves historical-candle coverage metadata alongside
the realized-volatility window.

Provenance: the public [IV-minus-realized-volatility discussion on
X](https://x.com/isellpremium/status/2072350364385349678) is treated as a
research lead and cross-checked against the [Bitcoin-options risk-premia
paper](https://papers.ssrn.com/sol3/Delivery.cfm/98257442-0b56-4c20-8b8f-c91befac0b1b-MECA.pdf?abstractid=6771170).
Neither source is treated as a performance guarantee.

## 中文

这一系列把可观察的期权曲面特征与不可观察的持仓归属分开。Skew 和期限结构使用透明的
moneyness 分桶；VRP 比较期权 ATM IV 与永续已实现波动率；Gamma map 使用无符号的
`gamma × OI × underlying²` 质量。Deribit summary 可能不返回 greeks，因此 gamma 监控会
通过有上限的 `/options/deribit/book` 补齐并报告已抓取/未抓取覆盖率。任何案例都不推断
做市商 gamma 多空、期权 PnL、对冲比率或执行结果。

VRP 的 recorder/replay 会先把 IV 减 RV 的快照冻结，再检验同一到期标识下“隐含波动率溢价”
状态是否持续。它只是波动率状态诊断，不是卖波动率建议；到期、RV 窗口、动态对冲、成交和成本
都没有被伪装成已匹配。

出处：公开的 [IV 减已实现波动率 X 讨论](https://x.com/isellpremium/status/2072350364385349678)
只是研究线索，并对照了 [Bitcoin options 风险溢价论文](https://papers.ssrn.com/sol3/Delivery.cfm/98257442-0b56-4c20-8b8f-c91befac0b1b-MECA.pdf?abstractid=6771170)。
两者都不被当作收益保证。

## Commands / 命令

```bash
MARKETBRIDGE_CONFIG=./config.options-research.example.yaml cargo run
python3 examples/crypto/options/crypto_options_gamma_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 --max-book-fetches 24
python3 examples/crypto/options/crypto_options_gamma_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-gamma.jsonl
python3 examples/crypto/options/crypto_options_gamma_replay.py \
  --input work/crypto-options-gamma.jsonl --min-run 3
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30
python3 examples/crypto/options/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto/options/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-run 3
python3 examples/crypto/options/crypto_options_vrp_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --price-exchange binance --symbol BTCUSDT --interval 1h --rv-bars 168
python3 examples/crypto/options/crypto_options_vrp_recorder.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --price-exchange binance --symbol BTCUSDT --interval 1h --rv-bars 168 \
  --iterations 20 --interval-secs 30 --output work/crypto-options-vrp.jsonl
python3 examples/crypto/options/crypto_options_vrp_replay.py \
  --input work/crypto-options-vrp.jsonl --vrp-threshold 5 --min-run 3
```
