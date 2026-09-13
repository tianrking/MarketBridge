# Options and volatility / 期权与波动率

## English

This family separates observable surface features from unobservable position
ownership. Skew and term structure use transparent moneyness buckets; VRP
compares option ATM IV with perp realized volatility; gamma maps use unsigned
`gamma × OI × underlying²` mass. Deribit summary rows may omit greeks, so the
gamma monitor uses bounded `/options/deribit/book` enrichment and reports
fetched/unfetched coverage. No example infers dealer gamma sign, option PnL,
hedge ratios or execution.

## 中文

这一系列把可观察的期权曲面特征与不可观察的持仓归属分开。Skew 和期限结构使用透明的
moneyness 分桶；VRP 比较期权 ATM IV 与永续已实现波动率；Gamma map 使用无符号的
`gamma × OI × underlying²` 质量。Deribit summary 可能不返回 greeks，因此 gamma 监控会
通过有上限的 `/options/deribit/book` 补齐并报告已抓取/未抓取覆盖率。任何案例都不推断
做市商 gamma 多空、期权 PnL、对冲比率或执行结果。

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
python3 examples/crypto/options/crypto_options_vrp_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --price-exchange binance --symbol BTCUSDT --interval 1h --rv-bars 168
```
