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

`crypto_options_term_structure_replay.py` is the separate time-series case for
the near/far ATM-IV slope already emitted by the skew recorder. It tests
whether an upward (contango) or inverted term-structure state persists for a
minimum run. Expiry identities can roll, so a persistent state is evidence to
investigate, not a calendar-spread or option trade.

The bull-call-spread monitor selects two calls from one expiry near configurable
moneyness targets, uses the lower call ask and higher call bid when available
(mark fallback is labeled), and reports debit, width, breakeven and capped
paper payoff geometry. Its recorder/replay asks whether the same expiry/strike
identity remains observable for a consecutive run. This is a quote-structure
hypothesis, not a recommendation: it does not model settlement, margin,
exercise, fills, fees, slippage, hedging or forward returns.

The bull-call-spread response recorder additionally freezes a synchronized
MarketBridge BTC quote. Its replay compares fixed-record BTC signed and
absolute returns after `bull_call_spread_quote_available` or
`bull_call_spread_mark_only` snapshots against snapshots where the spread was
not validated. This tests the public flow lead without converting it into an
option PnL, hedge or execution claim.

The gamma-response recorder joins each unsigned gamma snapshot to a
MarketBridge BTC quote. Its replay asks a falsifiable, non-directional question:
does a near-spot, concentrated gamma map have a different fixed-record forward
return or absolute-move distribution than other snapshots? It measures signed
and absolute spot responses only; it does not infer dealer gamma sign, option
PnL, hedge demand or a volatility-capture trade.

Provenance: the public [IV-minus-realized-volatility discussion on
X](https://x.com/isellpremium/status/2072350364385349678) is treated as a
research lead and cross-checked against the [Bitcoin-options risk-premia
paper](https://papers.ssrn.com/sol3/Delivery.cfm/98257442-0b56-4c20-8b8f-c91befac0b1b-MECA.pdf?abstractid=6771170).
Neither source is treated as a performance guarantee.
The term-structure definition is cross-checked against [Deribit Insights'
options data guide](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/),
which describes ATM implied volatility across different expiration dates. This
example deliberately uses MarketBridge's transparent near/far ATM buckets and
does not claim a complete interpolated surface.
The spread case preserves the public [Deribit bull-call-spread/options-flow
observation on X](https://x.com/laevitas1/status/1985373005644476891) as an
unverified research lead; the monitor makes leg-selection and quote-quality
assumptions explicit instead of inferring a profitable trade.
The gamma response lead preserves a public [gamma-wall discussion on X](https://x.com/david_eng_mba/status/2042265877488533758)
and cross-checks the observable option-book fields against [Deribit's public
market-data documentation](https://docs.deribit.com/api-reference/market-data/public-get-order-book).
Those sources motivate a testable concentration/response comparison, not a
claim that gamma walls predict direction or volatility.

## 中文

这一系列把可观察的期权曲面特征与不可观察的持仓归属分开。Skew 和期限结构使用透明的
moneyness 分桶；VRP 比较期权 ATM IV 与永续已实现波动率；Gamma map 使用无符号的
`gamma × OI × underlying²` 质量。Deribit summary 可能不返回 greeks，因此 gamma 监控会
通过有上限的 `/options/deribit/book` 补齐并报告已抓取/未抓取覆盖率。任何案例都不推断
做市商 gamma 多空、期权 PnL、对冲比率或执行结果。

VRP 的 recorder/replay 会先把 IV 减 RV 的快照冻结，再检验同一到期标识下“隐含波动率溢价”
状态是否持续。它只是波动率状态诊断，不是卖波动率建议；到期、RV 窗口、动态对冲、成交和成本
都没有被伪装成已匹配。

`crypto_options_term_structure_replay.py` 单独检验 skew recorder 已输出的近端/远端 ATM IV
斜率是否持续为升水（contango）或倒挂。到期标识会滚动，因此“持续”只是值得继续研究的
曲面状态证据，不是日历价差或期权交易指令。

牛市看涨价差监控会在同一到期日内，按可配置的 moneyness 目标挑选较低和较高执行价的看涨期权；
优先使用低执行价 ask 与高执行价 bid，缺失时才回退到并明确标记 mark。输出 debit、价差宽度、
盈亏平衡点和封顶的纸面收益几何；recorder/replay 检验相同到期日/执行价组合是否连续出现。
这只是报价结构假设，不是交易建议：没有伪装成已建模的结算、保证金、行权、成交、手续费、滑点、对冲或未来收益。

bull-call-spread response recorder 还会冻结同步的 MarketBridge BTC 报价；replay 比较
`bull_call_spread_quote_available` 或 `bull_call_spread_mark_only` 快照与未验证价差快照之后固定记录窗口的 BTC
有符号/绝对收益。它只检验公开期权流线索，不把结果转成期权 PnL、对冲或执行结论。

Gamma-response recorder 会把每次无符号 Gamma 快照和 MarketBridge 的 BTC 行情同时冻结。
Replay 检验一个可证伪的非方向性问题：近现货且集中于单一执行价的 Gamma map，未来固定记录窗口的
有符号收益或绝对波动分布，是否不同于其他快照。它只测现货响应，不推断做市商 Gamma 多空、期权
PnL、对冲需求或波动率交易。

出处：公开的 [IV 减已实现波动率 X 讨论](https://x.com/isellpremium/status/2072350364385349678)
只是研究线索，并对照了 [Bitcoin options 风险溢价论文](https://papers.ssrn.com/sol3/Delivery.cfm/98257442-0b56-4c20-8b8f-c91befac0b1b-MECA.pdf?abstractid=6771170)。
两者都不被当作收益保证。
期限结构定义对照了 [Deribit Insights 的期权数据说明](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/)，
其中将期限结构描述为不同到期日的 ATM 隐含波动率。这里仅使用 MarketBridge 透明的近端/远端 ATM 分桶，
不声称已经完成全曲面插值。
价差案例保留公开的 [Deribit 牛市看涨价差/期权流 X 观察](https://x.com/laevitas1/status/1985373005644476891)
作为未经验证的研究线索；实现会把选腿规则和报价质量写入输出，不把它推断成可获利交易。
Gamma 响应案例保留公开的 [X 上 gamma wall 讨论](https://x.com/david_eng_mba/status/2042265877488533758)，
并对照 [Deribit 公开市场数据文档](https://docs.deribit.com/api-reference/market-data/public-get-order-book)
中的期权盘口字段。出处只用于提出“集中度与后续响应是否有关”的可验证假设，不表示 Gamma wall 能预测方向或波动率。

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
python3 examples/crypto/options/crypto_options_gamma_response_recorder.py \
  --currency BTC --venue deribit --price-symbol BTCUSDT --exchange binance \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-gamma-response.jsonl
python3 examples/crypto/options/crypto_options_gamma_response_replay.py \
  --input work/crypto-options-gamma-response.jsonl --horizon-records 3 \
  --min-near-share 0.50 --min-concentration 0.10 --min-observations 5
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30
python3 examples/crypto/options/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto/options/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-run 3
python3 examples/crypto/options/crypto_options_term_structure_replay.py \
  --input work/crypto-options-skew.jsonl --min-slope-iv 3 --min-run 3
python3 examples/crypto/options/crypto_options_bull_call_spread_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --long-moneyness 0.95 --short-moneyness 1.05
python3 examples/crypto/options/crypto_options_bull_call_spread_recorder.py \
  --currency BTC --venue deribit --expiry-days 30 --iterations 20 --interval-secs 30 \
  --output work/crypto-options-bull-call-spread.jsonl
python3 examples/crypto/options/crypto_options_bull_call_spread_replay.py \
  --input work/crypto-options-bull-call-spread.jsonl --min-run 3
python3 examples/crypto/options/crypto_options_bull_call_spread_response_recorder.py \
  --currency BTC --venue deribit --price-exchange binance --price-symbol BTCUSDT \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-bull-call-spread-response.jsonl
python3 examples/crypto/options/crypto_options_bull_call_spread_response_replay.py \
  --input work/crypto-options-bull-call-spread-response.jsonl \
  --horizon-records 3 --min-observations 5
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
