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

`crypto_options_vrp_response_replay.py` reuses the VRP recorder archive and
compares the next fixed-record BTC response after implied-volatility-premium,
realized-volatility-above-implied and aligned states. It reports signed and
absolute movement without turning an IV-RV spread into a short-volatility
position. The selected option expiry may roll between snapshots, so expiry
identity is retained as metadata rather than treated as a matched contract.

`crypto_options_term_structure_replay.py` is the separate time-series case for
the near/far ATM-IV slope already emitted by the skew recorder. It tests
whether an upward (contango) or inverted term-structure state persists for a
minimum run. Expiry identities can roll, so a persistent state is evidence to
investigate, not a calendar-spread or option trade.

`crypto_options_term_structure_response_replay.py` reuses the skew-response
archive, joins each term-structure state to its BTC quote, and compares later
signed and absolute movement after upward, inverted and flat states. It is a
descriptive surface-response study, not a calendar-spread PnL or hedge model.

The skew-response recorder pairs the target-expiry wing-IV snapshot with a
synchronized MarketBridge BTC quote. Its replay compares fixed-record BTC
signed and absolute returns after `downside_protection_demand`,
`upside_call_demand` and `balanced_wing_iv` states. This is a descriptive
response study: moneyness buckets are not a universal 25-delta surface, and
the output is not an option PnL, hedge or execution signal. The skew monitor
also accepts `--bucket-mode delta` to use provider `delta` greeks for ATM and
25-delta wings when available; missing greeks stay out of the comparable
sample rather than falling back silently.

Provenance: the public [options brief on X](https://x.com/Gate_Launch/status/2063810805552845140)
is an unverified research lead. Delta-field semantics are cross-checked against
[Binance's public options market-data documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data);
the monitor does not claim a complete cross-venue surface or executable skew.

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

`crypto_historical_volatility_response_replay.py` consumes Bybit's public
option historical-volatility series and pairs each hourly provider observation
with a bounded Binance perpetual price window. It tests whether high or low
provider volatility is followed by a different absolute movement distribution
than ordinary observations. The provider value is not treated as implied
volatility, a forecast, option PnL or a trade instruction.

`crypto_deribit_volatility_index_response_replay.py` adds the separate Deribit
volatility-index input. It classifies public index closes into configurable
high, low and ordinary states, joins them to a fixed Binance price horizon, and
compares absolute BTC responses against the ordinary bucket. It deliberately
does not treat the index as a complete surface, a forecast, an option position
or an execution signal.

`crypto_deribit_volatility_index_vrp_response_replay.py` extends that case with
an explicit close-to-close realized-volatility window. It compares
index-minus-RV premium, discount and aligned states against later absolute BTC
responses, while keeping the different construction, annualization horizon and
missing-history assumptions visible. It is not a short-volatility or hedge
backtest.

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
The historical-volatility response lead is informed by the public [realized-volatility regime observation from Glassnode on X](https://x.com/glassnode/status/1955218957490594099)
and uses Bybit's first-party [Get Historical Volatility API](https://bybit-exchange.github.io/docs/v5/market/iv),
which documents hourly option historical-volatility values and bounded time windows.
The X post is a research lead, not a performance claim; provider volatility is
kept separate from implied volatility and no option position is modeled.
The Deribit index response lead is cross-checked against Deribit's official
[`public/get_volatility_index_data` documentation](https://docs.deribit.com/api-reference/market-data/public-get_volatility_index_data),
which defines the public OHLC candle fields, supported resolutions and bounded
timestamps. The index remains provider context; no volatility trade is modeled.
The index-minus-RV extension follows the public [volatility-risk-premium research lead on X](https://x.com/ConcretumR/status/1952298941745172695)
as a falsifiable comparison only; the public post is not treated as a crypto
performance claim.

## 中文

这一系列把可观察的期权曲面特征与不可观察的持仓归属分开。Skew 和期限结构使用透明的
moneyness 分桶；VRP 比较期权 ATM IV 与永续已实现波动率；Gamma map 使用无符号的
`gamma × OI × underlying²` 质量。Deribit summary 可能不返回 greeks，因此 gamma 监控会
通过有上限的 `/options/deribit/book` 补齐并报告已抓取/未抓取覆盖率。任何案例都不推断
做市商 gamma 多空、期权 PnL、对冲比率或执行结果。

VRP 的 recorder/replay 会先把 IV 减 RV 的快照冻结，再检验同一到期标识下“隐含波动率溢价”
状态是否持续。它只是波动率状态诊断，不是卖波动率建议；到期、RV 窗口、动态对冲、成交和成本
都没有被伪装成已匹配。

`crypto_options_vrp_response_replay.py` 复用 VRP recorder 归档，比较隐含波动率溢价、已实现波动率高于隐含和
两者接近状态之后固定记录窗口的 BTC 有符号/绝对响应。它不会把 IV-RV 差值转成卖波动率仓位；采样期间目标期权到期日
可能滚动，因此只把到期标识作为审计元数据，不假设合约已经匹配。

`crypto_options_term_structure_replay.py` 单独检验 skew recorder 已输出的近端/远端 ATM IV
斜率是否持续为升水（contango）或倒挂。到期标识会滚动，因此“持续”只是值得继续研究的
曲面状态证据，不是日历价差或期权交易指令。

`crypto_options_term_structure_response_replay.py` 复用 skew-response 归档，把期限结构状态与 BTC 报价配对，比较
升水、倒挂和平坦状态之后的有符号/绝对波动。这是曲面响应研究，不是日历价差 PnL 或对冲模型。

skew-response recorder 会把目标到期日的翼部 IV 快照与同步 MarketBridge BTC 报价配对；replay 比较
`downside_protection_demand`、`upside_call_demand` 和 `balanced_wing_iv` 状态之后固定记录窗口的 BTC
有符号/绝对收益。这只是描述性响应研究：moneyness 分桶不等于通用 25-delta 曲面，也不是期权 PnL、对冲或执行信号。
skew monitor 另支持 `--bucket-mode delta`，在 provider 暴露 `delta` greeks 时使用 ATM 和 25-delta 翼部；
缺少 greeks 的合约会留在可比样本之外，不会静默回退成另一种语义。

出处：公开的 [期权市场简报 X 线索](https://x.com/Gate_Launch/status/2063810805552845140)
只作为未经验证的研究假设；字段语义对照 [Binance 官方期权市场数据文档](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data)。
这不代表完整跨交易所曲面或可执行 skew。

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
`crypto_historical_volatility_response_replay.py` 使用 Bybit 公开的期权历史波动率，
把每个小时的提供方波动率观测和有界 Binance 永续价格窗口配对，检验高/低波动率状态之后的绝对波动分布
是否不同于普通状态。这里的 provider volatility 不是隐含波动率、预测、期权 PnL 或交易指令。

`crypto_deribit_volatility_index_response_replay.py` 使用新增的 Deribit 公开波动率指数历史接口，
把指数收盘值分成可配置的高、低和普通状态，再与固定窗口的 Binance BTC 价格响应配对，
只比较绝对波动分布。它不会把指数当成完整曲面、预测、期权仓位或执行信号。

`crypto_deribit_volatility_index_vrp_response_replay.py` 在此基础上加入明确的收盘价已实现波动率窗口，
比较指数减 RV 的溢价、折价和对齐状态之后的 BTC 绝对波动。不同的波动率构造、年化窗口和历史缺失会保留在结果中，
不会伪装成卖波动率或对冲回测。

出处：研究线索参考 [Glassnode 在 X 上的实现波动率状态观察](https://x.com/glassnode/status/1955218957490594099)，
数据字段和小时频率以 Bybit 官方 [Get Historical Volatility API](https://bybit-exchange.github.io/docs/v5/market/iv)
为准。X 内容不被当作收益证明，跨交易所价格只是响应对照。
Deribit 指数字段、分辨率和时间窗口对照官方 [`public/get_volatility_index_data` 文档](https://docs.deribit.com/api-reference/market-data/public-get_volatility_index_data)；
它仍然只是提供方波动率上下文，不构造卖波动率、对冲或执行路径。
指数减 RV 的扩展参考 [公开波动率风险溢价 X 线索](https://x.com/ConcretumR/status/1952298941745172695)，
只用于提出可证伪比较，不把该帖子当作加密收益证明。

## Commands / 命令

```bash
MARKETBRIDGE_CONFIG=./config.options-research.example.yaml cargo run
python3 examples/crypto/options/crypto_historical_volatility_response_replay.py \
  --base-coin BTC --quote-coin USD --period 30 --days 30 \
  --price-symbol BTCUSDT --price-interval 1h \
  --low-threshold-pct 25 --high-threshold-pct 50 \
  --horizon-bars 3 --min-observations 5
python3 examples/crypto/options/crypto_deribit_volatility_index_response_replay.py \
  --currency BTC --resolution 3600 --days 30 \
  --price-symbol BTCUSDT --price-interval 1h \
  --low-threshold 25 --high-threshold 75 \
  --horizon-bars 3 --min-observations 5
python3 examples/crypto/options/crypto_deribit_volatility_index_vrp_response_replay.py \
  --currency BTC --resolution 3600 --days 30 --price-symbol BTCUSDT \
  --price-interval 1h --rv-bars 24 --vrp-threshold 5 \
  --horizon-bars 3 --min-observations 5
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
python3 examples/crypto/options/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --bucket-mode delta --delta-band 0.05
python3 examples/crypto/options/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto/options/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-run 3
python3 examples/crypto/options/crypto_options_skew_response_recorder.py \
  --currency BTC --venue deribit --price-exchange binance --price-symbol BTCUSDT \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew-response.jsonl
python3 examples/crypto/options/crypto_options_skew_response_replay.py \
  --input work/crypto-options-skew-response.jsonl --horizon-records 3 \
  --min-skew-iv 3 --min-observations 5
python3 examples/crypto/options/crypto_options_term_structure_replay.py \
  --input work/crypto-options-skew.jsonl --min-slope-iv 3 --min-run 3
python3 examples/crypto/options/crypto_options_term_structure_response_replay.py \
  --input work/crypto-options-skew-response.jsonl --horizon-records 3 \
  --min-slope-iv 3 --min-observations 5
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
python3 examples/crypto/options/crypto_options_vrp_response_replay.py \
  --input work/crypto-options-vrp.jsonl --vrp-threshold 5 \
  --horizon-records 3 --min-observations 5
```
