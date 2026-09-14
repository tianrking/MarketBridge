# Carry and funding / 套利与资金费率

## English

This family studies relative prices and funding transfers without assuming a
fixed convergence date or executable hedge. The basis monitor joins spot/perp
quotes with current funding and only calls the result a research candidate
when the snapshot is fresh and the funding interval is known. The convergence
monitor/replay compares the same symbol across venues using explicit
point-in-time intervals. Missing intervals, borrow, transfer latency, margin,
fees and slippage remain evidence gaps, never zeros.

The convergence replay accepts `--paper-cost-bps-per-hour` and
`--min-net-spread-bps-per-hour`. These are explicit sensitivity hurdles: the
replay reports gross and after-cost differential persistence, but does not claim
they are exchange fees, borrow rates or executable hedge PnL.
It also carries each venue's historical funding `coverage_detail` into the
result, so a short provider page cannot be mistaken for a complete differential
window.

The basis recorder/replay is a separate falsifiable test: after a same-venue
basis observation is at least `min_z` standard deviations from its trailing
mean, does the absolute basis contract over the next `horizon` snapshots? It
reports contraction frequency and does not call that frequency carry PnL.

`crypto_funding_regime_replay.py` adds a funding-only persistence test: after
`min_run` consecutive observations remain beyond `min_funding_pct` in one
direction, does the next fixed price window move against that crowding proxy?
Positive funding is only a crowded-long proxy and negative funding only a
crowded-short proxy. The output is a forward-return hit rate, not funding
income, hedge PnL or an execution instruction. `funding_extremes.py` remains a
current-universe filter, while `funding_curve_demo.py` is a visualization
utility; neither is a backtest.

`crypto_positioning_regime_replay.py` joins funding, aggregate OI and perp price
at point-in-time timestamps, then reports the forward-return distribution for
each `price × OI × funding` regime. It is a state-matrix diagnostic, not a
long/short classifier: OI ownership, fills and hedge PnL remain unknown.

`crypto_oi_impulse_response_recorder.py` / `crypto_oi_impulse_response_replay.py`
are a separate temporal test of the narrower claim that an unusually large OI
expansion can be followed by larger absolute price movement (liquidation-risk
context), regardless of direction. The recorder only requests current OI and a
perpetual quote, while replay compares expansion, contraction and ordinary
snapshots at a fixed record-count horizon. It does not infer who is long or
short and does not turn expansion into a trade signal.

`crypto_funding_cross_section_replay.py` is a different case: it ranks a
caller-selected asset universe by point-in-time funding, keeps only fresh
funding observations, and compares the next-window returns of the lowest- and
highest-funding groups. It reports the low-minus-high spread with an optional
paper hurdle; it does not turn the ranking into a portfolio or hedge.

`crypto_funding_spread_response_replay.py` closes a different research gap. It
annualizes each venue's historical funding rate using its point-in-time
interval, aligns the two fresh series, and compares the next fixed BTC absolute
return after an extreme or shocked cross-venue spread with ordinary spread
windows. The spread is a carry-context/stress feature, not a direction signal;
the result does not estimate funding cash flow or claim a hedge can be filled.

`crypto_premium_funding_response_replay.py` uses Binance's historical premium
index, funding-rate points and perpetual candles to test a narrower market-data
hypothesis: when premium and funding have material opposite signs, is the next
fixed-horizon absolute or signed response different from ordinary observations?
Funding is carried only from the latest point within a bounded age window, and
missing alignment stays observe-only. This is not an executable carry trade,
funding-income estimate or directional signal.

`crypto_cross_venue_price_gap_replay.py` isolates same-asset price
fragmentation: it aligns two venue candle series, detects an extreme log-price
gap relative to a frozen trailing mean, and measures subsequent contraction.
The result is intentionally not called an arbitrage opportunity because
simultaneous bid/ask fills, inventory, transfers and venue solvency are not
observed by this replay.

`crypto_cross_venue_orderbook_monitor.py` is a narrower snapshot case: it
consumes both venues' asks and bids, computes target-notional VWAP on each side,
rejects excessive timestamp skew, and subtracts a caller-supplied paper cost.
The recorder/replay tests whether a qualifying book edge persists across
consecutive snapshots. It still does not model prefunded inventory, settlement,
queue position, transfer fees or execution.

The order-book response recorder adds a synchronized MarketBridge BTC quote;
its replay compares later signed and absolute BTC returns after a qualifying
book edge versus unqualified snapshots. This is a response study, not a
simultaneous fill, arbitrage PnL or routing model.

`crypto_triangular_arbitrage_monitor.py` is the single-venue three-leg analogue:
it requests the synchronized `BTCUSDT`, `ETHBTC` and `ETHUSDT` spot top-of-book,
calculates both USDT cycle directions, and applies a paper per-leg cost. The
recorder/replay pair asks whether a net edge survives consecutive snapshots.
This is a quote-consistency experiment, not a triangular-arbitrage execution
claim: depth, atomicity, fees, latency, inventory and partial fills are absent.

The triangular response recorder adds a synchronized MarketBridge BTC quote;
its replay compares later signed and absolute BTC returns after qualifying
three-leg edges versus unqualified snapshots. This is a response study, not a
route, atomic-fill, triangular PnL or execution model.

Provenance: the basis tests are motivated by the public [CryptoCred basis-trade
discussion on X](https://x.com/CryptoCred/status/1777720296297975952) and the
[CME-versus-spot basis example](https://x.com/0xscarlettw/status/1944584946670276938).
The funding persistence lead is cross-checked against the primary [Kraken
funding-rate strategy explanation](https://www.kraken.com/learn/futures-trading-funding-rate-strategy),
which describes funding as a positioning/crowding measure, and MarketBridge's
explicit funding schedule. These are research leads, not verified performance
claims.
The cross-venue differential lead is also informed by this public [funding
spread discussion on X](https://x.com/leondoteth/status/2012127303850213817).
The regime-matrix lead is informed by the public [OI/funding/price context
brief on X](https://x.com/ImCryptOpus/status/1949195275903410571).
The OI-impulse decomposition is informed by the public [XWIN note that
aggressive OI growth during rebounds can become fuel for another liquidation](https://x.com/xwinfinance/status/2023155692916646257)
and cross-checked against [Binance's open-interest history documentation](https://developers.binance.com/zh-CN/docs/catalog/core-trading-derivatives-trading-coin-futures/api/rest-api/market-data).
These sources motivate a volatility-response test, not a directional or
liquidation forecast.
The cross-sectional funding lead is also informed by the public [cross-venue
funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
and cross-checked against [Binance's official funding-history API documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
The spread-response decomposition uses the same public [cross-venue funding
spread discussion on X](https://x.com/leondoteth/status/2012127303850213817),
and preserves each venue's interval semantics using [Binance's funding-history
documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)
and [OKX's funding-rate-history documentation](https://app.okx.com/docs-v5/zh/#rest-api-public-data-get-funding-rate-history).
Those sources document public funding observations; they do not establish a
profitable spread trade or a price-volatility forecast.
The premium/funding decomposition is motivated by the public [cross-venue
funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
and uses Binance's official [Premium Index Kline API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Premium-Index-Kline-Data).
The premium index is exchange-derived market context; the case does not infer
funding cash flow, trader intent or executable convergence.
The cross-venue gap decomposition is cross-checked against the academic
[Trading and Arbitrage in Cryptocurrency Markets](https://www.sciencedirect.com/science/article/pii/S0304405X19301746)
and [Arbitrage across different Bitcoin exchange venues](https://onlinelibrary.wiley.com/doi/10.1111/acfi.13102).
They motivate a price-fragmentation test, not an executable arbitrage claim.
The order-book case is cross-checked against [Binance's public order-book
documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data)
and the [cross-exchange arbitrage-friction study](https://academic.oup.com/rof/article/28/4/1345?guestAccessKey=50540e27-1995-48e8-bb51-6b93b219d2ad).
Those sources motivate measuring depth and settlement friction, not assuming a
snapshot edge is executable. The triangular case is cross-checked against
[Binance's official spot market-data API documentation](https://developers.binance.com/en/docs/products/spot/rest-api)
and the peer-reviewed [Wish or reality? On the exploitability of triangular
arbitrage in cryptocurrency markets](https://www.sciencedirect.com/science/article/pii/S154461232401537X).
They motivate a falsifiable top-of-book persistence test; they do not establish
that a displayed three-leg edge can be filled.
The data semantics are cross-checked against [Binance's official open-interest
history documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info),
which describes bounded historical OI observations rather than trader-side ownership.
The funding/OI replay now forwards coverage metadata for funding, OI and price
history into its evidence output, so sparse timestamp overlap is visible in the
case result.

Useful inputs:

- `/v1/market/basis`
- `/v1/market/perpetual-funding`
- `/v1/history/candles?candle_type=funding_rate`

## 中文

这一系列研究现货/永续相对价格和资金费率转移，不假设固定收敛日期，也不假设可成交的
对冲。基差监控只有在快照新鲜、资金费率结算间隔已知时才报告研究候选；跨交易所收敛
监控/回放使用逐点时间间隔比较同一标的。缺失间隔、借币、转账延迟、保证金、手续费和
滑点都保持为证据缺口，绝不会当成零值。

收敛回放支持 `--paper-cost-bps-per-hour` 和 `--min-net-spread-bps-per-hour`，用于显式纸面敏感性
门槛。它会同时输出 gross 与扣除该门槛后的差异持续性，但不会把门槛冒充交易所手续费、借贷成本或
可成交对冲 PnL。

基差录制/回放是独立的可证伪测试：当同一交易所的基差相对滚动均值偏离至少
`min_z` 个标准差后，未来 `horizon` 个快照的绝对基差是否收缩？输出的是收缩频率，
不会把它冒充成套利 PnL。

`crypto_funding_regime_replay.py` 进一步做只用资金费率的持续性检验：连续
`min_run` 个观测同方向超过 `min_funding_pct` 后，未来固定价格窗口是否朝拥挤一侧的
反方向移动？正费率只作为多头拥挤代理，负费率只作为空头拥挤代理；输出是未来收益方向命中率，
不是资金费收入、对冲 PnL 或执行指令。`funding_extremes.py` 只是当前市场筛选，
`funding_curve_demo.py` 只是可视化，二者都不是回测。

`crypto_positioning_regime_replay.py` 在逐点时间上连接资金费率、聚合 OI 和永续价格，输出每个
`价格 × OI × 资金费率` 状态的未来收益分布。它是状态矩阵诊断，不是多空分类器；OI 归属、成交和
对冲 PnL 仍然未知。

`crypto_funding_cross_section_replay.py` 是不同的横截面案例：按逐点资金费率给调用者选择的资产宇宙
排名，只保留新鲜费率，再比较低费率组与高费率组的下一窗口收益。它输出低减高的差异并允许加入纸面
门槛，但不会把排名变成组合或对冲。

`crypto_funding_spread_response_replay.py` 解决的是另一个研究缺口：按各交易所逐点资金费率结算间隔
年化，在两个交易所之间对齐新鲜数据，再比较极端或突然扩大的资金费率价差之后固定 BTC 窗口的绝对收益，
并与普通价差窗口对照。价差这里只是 carry 上下文/压力特征，不是方向信号；不会计算实际资金费收入，
也不会声称对冲一定可成交。

`crypto_premium_funding_response_replay.py` 使用 Binance 历史 premium index、资金费率点和永续 K 线，
检验更窄的市场数据假设：当 premium 与 funding 出现明显相反符号时，之后固定窗口的有符号或绝对收益，
是否不同于普通对齐观测？资金费率只在限定的最大陈旧时间内按最近点对齐，缺失对齐保持 observe-only。
这不是可执行的 carry 交易、资金费收入估算或方向信号。

`crypto_cross_venue_orderbook_monitor.py` 是更窄的盘口快照案例：读取两边 ask/bid，按目标名义金额计算
两边 VWAP，拒绝超出时间偏差阈值的快照，并扣除调用者提供的纸面双边成本。recorder/replay 再检验盘口
edge 是否连续出现。它仍不模拟预存库存、结算、队列位置、转账费或执行。

orderbook response recorder 还会记录同步的 MarketBridge BTC 报价；replay 比较 qualifying book edge 与普通快照之后
固定记录窗口的 BTC 有符号/绝对收益。这是响应研究，不是同时成交、套利 PnL 或路由模型。

`crypto_triangular_arbitrage_monitor.py` 是同一交易所的三腿报价一致性案例：请求同步的
`BTCUSDT`、`ETHBTC`、`ETHUSDT` 现货盘口，分别计算两个 USDT 换算方向，并扣除每腿纸面成本。
recorder/replay 再检验净 edge 是否连续出现。它只是可证伪的报价实验，不是三角套利成交声明；深度、原子性、
手续费、延迟、库存和部分成交都没有被假设为已知。

triangular response recorder 还会记录同步的 MarketBridge BTC 报价；replay 比较 qualifying 三腿 edge 与普通快照之后
固定记录窗口的 BTC 有符号/绝对收益。这是响应研究，不是路由、原子成交、三角 PnL 或执行模型。

出处：基差测试思路来自公开的 [CryptoCred 基差交易讨论](https://x.com/CryptoCred/status/1777720296297975952)
和 [CME 与现货基差示例](https://x.com/0xscarlettw/status/1944584946670276938)。资金费率持续性线索
另外对照了一级资料 [Kraken 资金费率策略说明](https://www.kraken.com/learn/futures-trading-funding-rate-strategy)，
以及 MarketBridge 返回的明确结算间隔。它们都是研究线索，不是已经验证的收益声明。
跨交易所差异线索也参考了公开的 [资金费率价差讨论](https://x.com/leondoteth/status/2012127303850213817)。
状态矩阵线索也参考了公开的 [OI/资金费率/价格上下文简报](https://x.com/ImCryptOpus/status/1949195275903410571)。
横截面资金费率线索也参考了公开的 [跨交易所资金费率差异讨论](https://x.com/leondoteth/status/2012127303850213817)，
并对照 [Binance 官方资金费率历史 API 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)。
价差响应拆解同样参考公开的 [跨交易所资金费率差异讨论](https://x.com/leondoteth/status/2012127303850213817)，
并以 [Binance 资金费率历史文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)
和 [OKX 资金费率历史文档](https://app.okx.com/docs-v5/zh/#rest-api-public-data-get-funding-rate-history)
保留各平台逐点结算间隔语义。这些资料只证明公开资金费率可观察，不证明价差交易有收益，也不证明它能预测价格波动。
premium/funding 拆解参考公开的 [跨交易所资金费率差异讨论](https://x.com/leondoteth/status/2012127303850213817)，
并使用 Binance 官方 [Premium Index Kline API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Premium-Index-Kline-Data)。
premium index 是交易所衍生的市场上下文，不代表资金费现金流、交易者意图或可执行收敛。

盘口案例对照 [Binance 公开 order-book 文档](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data)
以及[跨交易所套利摩擦研究](https://academic.oup.com/rof/article/28/4/1345?guestAccessKey=50540e27-1995-48e8-bb51-6b93b219d2ad)。
这些资料支持测量深度和结算摩擦，不支持把单次快照 edge 当成可执行机会。
三角案例对照 [Binance 官方现货市场数据 API 文档](https://developers.binance.com/en/docs/products/spot/rest-api)
以及同行评审的 [加密货币三角套利可利用性研究](https://www.sciencedirect.com/science/article/pii/S154461232401537X)。
这些资料支持做报价 edge 持续性检验，不支持把展示出的三腿差异当成可成交利润。

主要接口：

- `/v1/market/basis`
- `/v1/market/perpetual-funding`
- `/v1/history/candles?candle_type=funding_rate`

## Commands / 命令

```bash
python3 examples/crypto/carry/basis_carry_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/carry/funding_convergence_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit --iterations 3
python3 examples/crypto/carry/funding_convergence_replay.py \
  --symbol BTCUSDT --exchanges binance,bybit --days 7 --limit 200 \
  --paper-cost-bps-per-hour 0.25 --min-net-spread-bps-per-hour 0.5
python3 examples/crypto/carry/crypto_basis_recorder.py \
  --symbol BTCUSDT --exchanges binance,okx --iterations 120 --interval-secs 30 \
  --output work/crypto-basis.jsonl
python3 examples/crypto/carry/crypto_basis_replay.py \
  --input work/crypto-basis.jsonl --symbol BTCUSDT \
  --lookback 20 --horizon 3 --min-z 2.0
python3 examples/crypto/carry/crypto_funding_oi_replay.py \
  --symbol BTCUSDT --funding-exchange binance --oi-exchange binance \
  --price-exchange binance --days 7
python3 examples/crypto/carry/crypto_funding_regime_replay.py \
  --symbol BTCUSDT --funding-exchange binance --price-exchange binance \
  --days 14 --min-funding-pct 0.01 --min-run 3 --horizon-bars 3
python3 examples/crypto/carry/crypto_positioning_regime_replay.py \
  --symbol BTCUSDT --funding-exchange binance --oi-exchange binance \
  --price-exchange binance --days 7 --price-interval 5m \
  --lookback-bars 3 --horizon-bars 3 --min-observations 3
python3 examples/crypto/microstructure/crypto_oi_impulse_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --min-oi-change-pct 0.25 --output work/crypto-oi-impulse-response.jsonl
python3 examples/crypto/microstructure/crypto_oi_impulse_response_replay.py \
  --input work/crypto-oi-impulse-response.jsonl --horizon-records 7 \
  --min-oi-change-pct 0.25 --min-observations 5
python3 examples/crypto/carry/crypto_funding_cross_section_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --funding-exchange binance \
  --price-exchange binance --interval 1h --days 14 --top-k 1 \
  --min-dispersion-bps 1 --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/carry/crypto_funding_spread_response_replay.py \
  --symbol BTCUSDT --exchange-a binance --exchange-b bybit \
  --price-exchange binance --interval 1h --days 14 \
  --min-abs-spread-bps-per-year 1000 --shock-bps-per-year 0 \
  --horizon-bars 3 --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/carry/crypto_premium_funding_response_replay.py \
  --symbol BTCUSDT --exchange binance --interval 1h --days 14 \
  --premium-threshold-bps 1 --funding-threshold-bps 1 \
  --horizon-bars 3 --min-observations 5
python3 examples/crypto/carry/crypto_cross_venue_price_gap_replay.py \
  --exchange-a binance --exchange-b okx --symbol BTCUSDT --market spot \
  --interval 5m --lookback-bars 24 --horizon-bars 6 --entry-z 2 \
  --paper-cost-bps 10 --min-contraction-bps 0
python3 examples/crypto/carry/crypto_cross_venue_orderbook_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit --target-notional 10000 \
  --max-skew-ms 2000 --paper-cost-bps 20 --min-net-edge-bps 0
python3 examples/crypto/carry/crypto_cross_venue_orderbook_recorder.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit --iterations 120 --interval-secs 5 \
  --target-notional 10000 --paper-cost-bps 20 \
  --output work/crypto-cross-venue-orderbook.jsonl
python3 examples/crypto/carry/crypto_cross_venue_orderbook_replay.py \
  --input work/crypto-cross-venue-orderbook.jsonl --min-run 3 \
  --min-net-edge-bps 0
python3 examples/crypto/carry/crypto_cross_venue_orderbook_response_recorder.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit --market spot \
  --target-notional 10000 --paper-cost-bps 20 --min-net-edge-bps 0 \
  --iterations 120 --interval-secs 5 \
  --output work/crypto-cross-venue-orderbook-response.jsonl
python3 examples/crypto/carry/crypto_cross_venue_orderbook_response_replay.py \
  --input work/crypto-cross-venue-orderbook-response.jsonl \
  --horizon-records 3 --min-net-edge-bps 0 --min-observations 5
python3 examples/crypto/carry/crypto_triangular_arbitrage_monitor.py \
  --exchange binance --start-notional 10000 --max-skew-ms 500 \
  --paper-cost-bps-per-leg 10 --min-net-edge-bps 0
python3 examples/crypto/carry/crypto_triangular_arbitrage_recorder.py \
  --exchange binance --iterations 120 --interval-secs 2 \
  --output work/crypto-triangular-arbitrage.jsonl
python3 examples/crypto/carry/crypto_triangular_arbitrage_replay.py \
  --input work/crypto-triangular-arbitrage.jsonl --min-run 3 \
  --min-net-edge-bps 0
python3 examples/crypto/carry/crypto_triangular_arbitrage_response_recorder.py \
  --exchange binance --price-symbol BTCUSDT --iterations 120 --interval-secs 2 \
  --min-net-edge-bps 0 --output work/crypto-triangular-arbitrage-response.jsonl
python3 examples/crypto/carry/crypto_triangular_arbitrage_response_replay.py \
  --input work/crypto-triangular-arbitrage-response.jsonl \
  --horizon-records 3 --min-net-edge-bps 0 --min-observations 5
python3 examples/crypto/carry/funding_extremes.py \
  --exchange binance --min-pct -2 --max-pct -0.1
python3 examples/crypto/carry/funding_curve_demo.py \
  --symbol BTCUSDT --days 30 --no-png
```
