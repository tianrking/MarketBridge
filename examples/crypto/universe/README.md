# Universe and cross-asset research / 资产宇宙与跨资产研究

> **Language / 语言**: [English](README.en.md) · [简体中文](README.zh-CN.md)
>
> For the polished quickstart and boundary notes, start with the language-specific guide.

## English

These cases discover candidates rather than allocate capital. The universe
scanner joins liquidity, realized volatility and current funding. The
cross-asset replay ranks trailing returns across exact timestamp intersections
and compares the selected basket with an equal-weight benchmark over a fixed
horizon. Missing symbols and insufficient history remain visible.

`crypto_altcoin_breadth_replay.py` is a separate participation test. It counts
the fraction of caller-selected altcoins whose trailing return beats BTC, then
compares the next equal-weight altcoin-basket return with BTC by low, neutral and
high breadth states. This is intentionally an equal-count proxy: it does not
reproduce BlockchainCenter's market-cap-weighted Top-50 universe or claim to be
an official Altcoin Season Index.

`crypto_global_market_regime_monitor.py` adds a provider-level macro snapshot:
it classifies CoinGecko total-market change and BTC dominance as stress,
BTC-dominant risk-on, broad risk-on or mixed context. This is a regime label to
condition later breadth or momentum replays; one snapshot is not a historical
dominance factor, return forecast or allocation instruction.

`crypto_universe_opportunity_recorder.py` / `crypto_universe_opportunity_replay.py`
freeze the scanner's candidate sets and test whether the top-k universe remains
present across snapshots. This is persistence evidence only; it does not turn a
ranking into an allocation or execution signal.

`crypto_universe_opportunity_response_recorder.py` /
`crypto_universe_opportunity_response_replay.py` extend that archive with
perpetual quotes for the selected top-k symbols and a BTC benchmark. The replay
forms an equal-weight paper basket using only the point-in-time candidate list,
then compares its next fixed-record return with BTC. It is a falsifiable
candidate-discovery response study, not a portfolio or rebalancing instruction;
missing constituent prices are excluded from coverage rather than filled.

The volatility-adjusted replay is a separate ranking test: it divides each
asset's trailing return by its trailing per-bar realized volatility before
selecting the top basket. Zero-volatility assets are excluded rather than
assigned an infinite score. This tests risk-adjusted ranking; it does not
allocate capital or promise a Sharpe ratio.

`crypto_adaptive_cross_asset_replay.py` is the complementary signed-exposure
case. It keeps every asset's volatility-normalized score, scales signed weights
to a transparent gross cap, and sets the paper index to neutral when signals
conflict below a caller threshold. It compares that adaptive paper return with
an equal-weight basket; it is not a portfolio allocator or a prediction model.

Both the replay and sweep accept `--roundtrip-cost-bps` as a transparent paper
hurdle. It subtracts a fixed relative cost from the basket edge; it is not a
venue-specific fee, queue, fill or capacity model.

`crypto_volatility_adjusted_momentum_sweep.py` runs a bounded grid over the
lookback, volatility and forward-horizon windows after fetching each symbol
once. It exposes in-sample sensitivity and explicitly labels the best row as
descriptive only; a time-held-out, cost-aware replay is still required.

`crypto_volatility_adjusted_momentum_walkforward.py` performs that chronological
holdout for one selected parameter set. Warm-up bars before the split are used
only to form test features; the test score is never used to choose parameters.

`crypto_pairs_mean_reversion_replay.py` is a separate relative-value case. It
forms a caller-selected log-price spread, freezes its trailing mean and
standard deviation at each signal, and tests whether an extreme deviation
shrinks over a fixed future horizon. A fixed hedge ratio is a transparent
diagnostic parameter, not a cointegration certification or paired execution.

`crypto_universe_delist_risk_monitor.py` is a data-quality guard for all other
universe cases. It surfaces historical markets whose current quote is missing
or stale, but deliberately does not call that proof of delisting or perform an
automatic exclusion.

The aggregate Rust research-regime files live under `crypto/macro/`:
`crypto_market_regime_monitor.py` exposes the aggregate Rust research-regime
snapshot (fragmented, high-volatility, leveraged or normal) as Python JSON
context. Its guidance only tells a researcher which evidence checks deserve
attention; it does not choose or execute a strategy.

`crypto/macro/crypto_market_regime_recorder.py` / `crypto/macro/crypto_market_regime_replay.py` turn that
current context into a temporal study: the recorder freezes the regime beside a
BTC quote, and replay reports forward-return, absolute-move and downside
distributions by regime. The feature is still a current aggregate snapshot,
not a point-in-time historical factor or a strategy selector.

Provenance: [RoboNet's public multi-asset strategy discussion on X](https://x.com/RoboNetHQ/status/2024893544520143012)
motivates the volatility-adjusted comparison, while [CME's crypto
diversification study](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html)
documents that major crypto assets have materially different volatility. Both
are inputs to a falsifiable replay, not evidence of a guaranteed edge.
The adaptive case tests the post's observable claims about volatility-normalized
BTC/ETH/SOL signals, confidence-scaled exposure and an 8-hour horizon without
importing Allora predictions or Paradex execution.

The candidate-discovery framing is also informed by the public [multi-asset
perpetuals discussion by RoboNet](https://x.com/RoboNetHQ/status/2024893544520143012);
it is treated as an unverified research lead.

The breadth definition is cross-checked against [BlockchainCenter's official
Altcoin Season Index description](https://www.blockchaincenter.net/altcoin-season-index/),
which defines an alt season using the share of a Top-50 universe outperforming
Bitcoin over a 90-day window. MarketBridge exposes the universe, lookback and
thresholds as caller parameters so the approximation remains auditable.

The pair-reversion decomposition is cross-checked against the peer-reviewed
[Pairs Trading in Cryptocurrency Markets](https://ieeexplore.ieee.org/document/9200323/)
and the public [Pairs Trading in Crypto paper](https://papers.ssrn.com/sol3/Delivery.cfm/6188418.pdf?abstractid=6188418&mirid=1&type=2).
Those sources motivate a testable relative-price hypothesis; they are not a
performance or execution guarantee for MarketBridge.

The aggregate-regime framing is motivated by the unverified [XWIN trend and
positioning discussion on X](https://x.com/xwinfinance/status/2023155692916646257);
MarketBridge only measures the response distribution of its own explicit Rust
regime labels.
The global-context case is cross-checked against CoinGecko's official [Crypto
Global Market Data](https://docs.coingecko.com/reference/crypto-global) and
[market-research guidance](https://docs.coingecko.com/docs/market-research),
which document BTC dominance, total market cap/volume and active-market counts.
The public X lead is the unverified [BTC-dominance/alt-season discussion](https://x.com/1881Erdem/status/2041949708864643566);
it motivates a falsifiable context test, not a timing claim.

## 中文

这些案例用于发现候选，不负责分配资金。Universe scanner 连接流动性、已实现波动率和当前
资金费率；跨资产回放在共同 timestamp 上排名历史收益，并将选中篮子与固定窗口的等权基准
比较。缺失标的和历史长度不足都会保留在结果里。

`crypto_altcoin_breadth_replay.py` 是独立的市场参与度检验：统计调用者选择的山寨币中，过去窗口收益跑赢 BTC
的比例，再按低、中性、高 breadth 状态比较下一窗口等权山寨币篮子相对 BTC 的响应。这是等计数近似，
不会冒充 BlockchainCenter 的市值加权 Top-50 或官方 Altcoin Season Index。

波动率调整回放是独立的排名测试：先用历史收益除以逐 K 线已实现波动率，再选择排名靠前
的篮子。零波动标的会被排除，而不是赋予无穷大分数。它测试风险调整后的排名，不分配资金，
也不承诺 Sharpe 比率。

`crypto_adaptive_cross_asset_replay.py` 是配套的有符号敞口案例：保留每个资产的波动率标准化分数，
把有符号权重缩放到透明的总敞口上限；当信号冲突低于调用者阈值时，纸面指数收缩到 neutral。它将
这个自适应纸面收益与等权篮子对照，不是组合分配器或预测模型。

回放和扫描都支持 `--roundtrip-cost-bps` 透明纸面成本门槛：它从篮子 edge 中扣除固定相对
成本，但不是交易所费率、队列、成交或容量模型。

`crypto_volatility_adjusted_momentum_sweep.py` 在只请求一次每个标的历史数据后，扫描回看、
波动率和前瞻窗口的有限网格。它用于暴露样本内敏感性，并明确把最佳行标记为描述性结果；
仍需时间切分、成本感知的样本外回放。

`crypto_volatility_adjusted_momentum_walkforward.py` 对选定参数执行按时间排序的训练/测试
切分。测试段只使用切分前的预热 K 线和切分后的当前数据，测试结果不会反过来挑参数。

`crypto_pairs_mean_reversion_replay.py` 是独立的相对价值案例：构造调用者选择的对数价格价差，
在每个信号时冻结滚动均值和标准差，检验极端偏离是否在固定未来窗口收缩。固定 hedge ratio 只是
透明诊断参数，不是协整证明或配对执行。

`crypto_universe_opportunity_recorder.py` / `crypto_universe_opportunity_replay.py` 会冻结 scanner
候选集合，检验 top-k 标的是否跨快照持续出现。这只是候选持续性证据，不会把排名变成资金分配或执行信号。

`crypto_universe_opportunity_response_recorder.py` /
`crypto_universe_opportunity_response_replay.py` 在同一归档中补充 top-k 标的永续报价和 BTC 基准。
Replay 只使用当时的候选列表构造等权纸面篮子，再比较下一固定记录窗口相对 BTC 的收益。这是可证伪的候选响应研究，
不是组合或再平衡指令；成分报价缺失会保留为覆盖缺口，不会填成零。

出处：[RoboNet 在 X 的多资产策略讨论](https://x.com/RoboNetHQ/status/2024893544520143012)
提供了波动率调整的研究线索；[CME 的加密资产分散研究](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html)
说明主要加密资产的波动率确实不同。两者只是可证伪回放的输入，不代表保证收益。
自适应案例只测试该帖子关于波动率标准化 BTC/ETH/SOL 信号、置信度调整敞口和 8 小时窗口的可观察部分，
不接入 Allora 预测，也不接入 Paradex 执行。

breadth 定义对照 [BlockchainCenter 的 Altcoin Season Index 说明](https://www.blockchaincenter.net/altcoin-season-index/)，
该说明使用 Top-50 中跑赢 Bitcoin 的比例和 90 天窗口。MarketBridge 将币篮子、回看窗口和阈值都交给调用者，
确保这个近似可审计。

配对回归拆解另外对照了同行评审的 [Pairs Trading in Cryptocurrency Markets](https://ieeexplore.ieee.org/document/9200323/)
和公开的 [Pairs Trading in Crypto 论文](https://papers.ssrn.com/sol3/Delivery.cfm/6188418.pdf?abstractid=6188418&mirid=1&type=2)。
这些资料只提供可测试的相对价格假设，不是 MarketBridge 的收益或执行保证。

`crypto_universe_delist_risk_monitor.py` 是其他 universe 案例前的数据质量护栏：它显示历史市场当前报价缺失或
过期，但不把这直接解释为退市证明，也不自动排除标的。

Rust 聚合研究状态文件位于 `crypto/macro/`：`crypto_market_regime_monitor.py` 将 Rust 聚合研究状态（fragmented、high_volatility、leveraged、normal）
作为 Python JSON 上下文输出。提示只告诉研究者应该检查哪些证据，不选择策略，也不执行交易。

`crypto/macro/crypto_market_regime_recorder.py` / `crypto/macro/crypto_market_regime_replay.py` 把当前上下文扩展成时间研究：记录状态和 BTC
报价，再按 regime 报告未来收益、绝对波动和下行比例。该特征仍是当前聚合快照，不是 point-in-time 历史因子，
也不是策略选择器。

`crypto_global_market_regime_monitor.py` 补充提供方级别的全市场宏观快照：根据 CoinGecko 总市值变化和 BTC
dominance 分类为 stress、BTC-dominant risk-on、broad risk-on 或 mixed。它用于给 breadth 或 momentum 回放
提供上下文；单次快照不是历史 dominance 因子、收益预测或资金分配指令。

`crypto_global_market_regime_recorder.py` / `crypto_global_market_regime_replay.py` 将该快照与同步的 BTC
报价写入 JSONL，并按固定记录窗口报告各 regime 的收益、绝对波动和下行比例；缺少历史 global 快照、精确时间间隔和
成交成本时保持为证据缺口。

聚合状态研究线索参考未经验证的 [XWIN 趋势与持仓讨论](https://x.com/xwinfinance/status/2023155692916646257)；
MarketBridge 只检验自己明确输出的 Rust regime 标签的响应分布。
全市场上下文对照 CoinGecko 官方 [Crypto Global Market Data](https://docs.coingecko.com/reference/crypto-global)
和[市场研究说明](https://docs.coingecko.com/docs/market-research)，其中明确列出 BTC dominance、总市值/成交量和活跃市场数量。
公开 X 的[BTC dominance/alt-season 讨论](https://x.com/1881Erdem/status/2041949708864643566)只作为未经验证的假设来源。

## Commands / 命令

```bash
python3 examples/crypto/universe/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m --min-score 2
python3 examples/crypto/universe/crypto_universe_opportunity_recorder.py \
  --exchange binance --market perp --interval 5m \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --min-score 2 \
  --iterations 30 --interval-secs 30 --output work/crypto-universe-opportunities.jsonl
python3 examples/crypto/universe/crypto_universe_opportunity_replay.py \
  --input work/crypto-universe-opportunities.jsonl --top-k 3 --min-run 3
python3 examples/crypto/universe/crypto_universe_opportunity_response_recorder.py \
  --exchange binance --market perp --interval 5m \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --min-score 2 --price-top-k 3 \
  --iterations 30 --interval-secs 30 \
  --output work/crypto-universe-opportunity-response.jsonl
python3 examples/crypto/universe/crypto_universe_opportunity_response_replay.py \
  --input work/crypto-universe-opportunity-response.jsonl \
  --top-k 3 --horizon-records 3 --min-assets 2 --min-observations 5
python3 examples/crypto/universe/crypto_cross_asset_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --horizon-bars 8 --top-k 1
python3 examples/crypto/universe/crypto_altcoin_breadth_replay.py \
  --btc-symbol BTCUSDT --alt-symbols ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT,ADAUSDT \
  --exchange binance --market perp --interval 1d --lookback-bars 90 \
  --horizon-bars 7 --low-threshold 0.25 --high-threshold 0.75 \
  --min-alt-assets 3 --min-observations 5 --paper-cost-bps 20
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 --top-k 1 \
  --roundtrip-cost-bps 20
python3 examples/crypto/universe/crypto_adaptive_cross_asset_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 \
  --min-net-exposure 0.10 --max-gross-exposure 1.0 \
  --roundtrip-cost-bps 20 --min-observations 5
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_sweep.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 4,8,12 --volatility-bars 4,8,12 \
  --horizon-bars 4,8 --top-k 1 --roundtrip-cost-bps 20
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_walkforward.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 8 --volatility-bars 8 --horizon-bars 8 \
  --train-fraction 0.7 --roundtrip-cost-bps 20
python3 examples/crypto/universe/crypto_pairs_mean_reversion_replay.py \
  --symbol-a BTCUSDT --symbol-b ETHUSDT --exchange binance --market perp \
  --interval 1h --lookback-bars 24 --horizon-bars 6 --entry-z 2 \
  --paper-cost-bps 10 --min-convergence-bps 0
python3 examples/crypto/universe/crypto_universe_delist_risk_monitor.py \
  --exchange binance --market perp --interval 1d \
  --stale-after-ms 86400000 --limit 100
python3 examples/crypto/macro/crypto_market_regime_monitor.py \
  --symbols BTCUSDT,ETHUSDT --intervals 1h,4h,1d
python3 examples/crypto/macro/crypto_market_regime_recorder.py \
  --symbols BTCUSDT,ETHUSDT --exchange binance --market perp \
  --intervals 1h,4h,1d --price-symbol BTCUSDT --iterations 30 \
  --interval-secs 30 --output work/crypto-market-regime.jsonl
python3 examples/crypto/macro/crypto_market_regime_replay.py \
  --input work/crypto-market-regime.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 10
python3 examples/crypto/universe/crypto_global_market_regime_monitor.py
python3 examples/crypto/universe/crypto_global_market_regime_recorder.py \
  --iterations 30 --interval-secs 600 \
  --output work/crypto-global-market-regime.jsonl
python3 examples/crypto/universe/crypto_global_market_regime_replay.py \
  --input work/crypto-global-market-regime.jsonl --horizon-records 3 \
  --min-observations 5 --paper-cost-bps 10
```
