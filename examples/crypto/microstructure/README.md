# Microstructure, squeeze and liquidation / 微结构、逼空与清算

> **Language / 语言**: [English](README.en.md) · [简体中文](README.zh-CN.md)
>
> For the polished quickstart and boundary notes, start with the language-specific guide.

## English

These observers combine funding, OI change, spot/perp order flow, book depth,
price context and liquidation events. They are confluence reports, not entry
signals. The first polling cycle intentionally has no OI change baseline. A
venue-specific liquidation side is not universal, so the replay keeps source
and coverage metadata visible and downgrades missing data to `observe_only`.

The ADL response pair treats Binance's public rating as provider context only.
It compares high, medium and low snapshots with later BTC signed and absolute
movement, but does not claim that an ADL event occurred or that the rating
predicts price. Binance describes the rating as a symbol-level measure that
incorporates insurance-fund balance, concentration, depth, volatility, leverage
and margin utilization; private account risk remains outside this repository.
Research provenance includes a public [liquidation discussion on X](https://x.com/angustias87/status/2039147109228925373)
and Binance's [ADL risk API documentation](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk).

Cases:

- `short_squeeze_monitor.py`: negative funding + rising OI + spot/perp flow divergence.
- `crypto_short_squeeze_response_recorder.py` / `crypto_short_squeeze_response_replay.py`: freeze that four-component score beside a quote and compare score-qualified snapshots with later fixed-record BTC responses.
- `exhaustion_short_monitor.py`: positive funding + failed highs + falling OI + weak bids.
- `liquidation_reversal_monitor.py`: sell-side liquidation + falling OI + positive CVD + recovery.
- `crypto_liquidation_burst_replay.py`: rolling liquidation-notional threshold versus forward absolute price movement.
- `crypto_liquidation_burst_response_recorder.py` / `crypto_liquidation_burst_response_replay.py`: freeze the rolling burst beside a quote and compare later BTC responses with ordinary snapshots.
- `crypto_liquidation_price_cluster_replay.py`: observed liquidation prints grouped into price bands, compared with ordinary forward absolute movement.
- `crypto_liquidation_price_cluster_response_recorder.py` / `crypto_liquidation_price_cluster_response_replay.py`: freeze observed price-band concentration beside a quote and compare later BTC movement.
- `crypto_liquidity_sandwich_monitor.py` / response recorder / replay: test whether symmetric near-touch bid/ask depth with a tight spread is followed by a different absolute BTC response than ordinary books.
- `crypto_microstructure_monitor.py`: top-of-book imbalance with funding context.
- `crypto_microstructure_response_recorder.py` / `crypto_microstructure_response_replay.py`: freeze imbalance/funding states beside a quote and compare later BTC responses.
- `crypto_flow_book_confirmation.py`: taker flow confirms or rejects L2 pressure.
- `crypto_cvd_divergence_replay.py`: tests whether a price move that disagrees with single-venue taker-flow delta is followed by a fixed-horizon reversal.
- `crypto_trade_imbalance_bar_replay.py`: closes event bars at a fixed quote-notional threshold and compares strong signed taker imbalance with balanced bars over the next event bars.
- `crypto_vpin_response_replay.py`: averages absolute signed imbalance across fixed-volume buckets and compares high-VPIN-proxy buckets with normal buckets by later absolute movement.
- `crypto_derivatives_sentiment_monitor.py`: reads optional CoinGlass funding/OI/long-short/liquidation context without treating aggregate metrics as ownership.
- `crypto_adl_risk_monitor.py`: observes Binance symbol-level high/medium/low ADL risk as liquidation-risk context, never as a directional signal.
- `crypto_adl_risk_response_recorder.py` / `crypto_adl_risk_response_replay.py`: freeze ADL-risk states beside a BTC quote and compare later fixed-record responses by provider-risk bucket.
- `crypto_derivatives_sentiment_recorder.py` / `crypto_derivatives_sentiment_replay.py`: freeze aggregate CoinGlass context and require consecutive crowding states before promoting persistence.
- `crypto_derivatives_crowding_response_recorder.py` / `crypto_derivatives_crowding_response_replay.py`: freeze the same context beside a price snapshot and compare signed fixed-record responses after long/short crowding, with a separate liquidation-qualified bucket.
- `crypto_spot_perp_depth_gap_monitor.py`: compares same-venue spot/perp target-size depth and impact.
- `crypto_spot_perp_depth_gap_recorder.py` / `crypto_spot_perp_depth_gap_replay.py`: test whether that gap persists across snapshots.
- `crypto_spot_perp_depth_gap_response_recorder.py` / `crypto_spot_perp_depth_gap_response_replay.py`: freeze depth-gap states beside a quote and compare later BTC movement by state.
- `crypto_volatility_breakout_replay.py`: compressed range plus volume confirmation versus forward returns.
- `crypto_bollinger_squeeze_replay.py`: a trailing BandWidth squeeze followed by an upper/lower-band break versus fixed-horizon continuation.
- `crypto_session_filter.py`: VWAP/EMA/MACD/volume session-window filter replay.
- `crypto_session_momentum_replay.py`: historical fixed-horizon test of the session VWAP/EMA/MACD/volume confluence.
- `crypto_weekday_hour_effect_replay.py`: matched-clock test of a selected UTC weekday/hour against other weekdays at the same hour.
- `crypto_vwap_deviation_reversion_replay.py`: prior UTC-session VWAP deviation followed by a cross-back versus fixed-horizon directional response.
- `crypto_anchored_vwap_replay.py`: prior swing-low/high anchored VWAP reclaim/rejection versus a fixed-horizon response.
- `crypto_volume_profile_breakout_replay.py`: tests whether an OHLCV-approximated low-volume-node breach continues over a fixed horizon.
- `crypto_atr_regime_response_replay.py`: separates compressed, ordinary and expanded ATR states and compares later signed, absolute and path-risk responses.
- `crypto_breakout_retest_response_replay.py`: tests a prior-range breakout followed by a bounded touch-and-reclaim retest against later aligned returns.
- `crypto_ichimoku_cloud_response_replay.py`: groups as-of cloud, Tenkan/Kijun and Chikou alignment states for later BTC response analysis.
- `crypto_rsi_bollinger_extreme_response_replay.py`: separates joint RSI/Bollinger extremes from one-indicator and ordinary states.
- `crypto_fibonacci_retracement_response_replay.py`: groups point-in-time 38.2%, 50% and 61.8% retracement zones against control ranges.
- `crypto_fair_value_gap_response_replay.py`: tests three-candle wick non-overlap zones, later touches/fills and ordinary-bar responses.
- `crypto_obv_divergence_response_replay.py`: compares close-signed volume-flow divergence, confirmation and mixed controls.
- `crypto_liquidity_sweep_response_replay.py`: tests whether a prior-range high/low sweep followed by a close reclaim and directional candle has a different aligned forward response.
- `crypto_footprint_imbalance_monitor.py` / recorder / replay: observes price-bin bid/ask delta and stacked imbalance persistence from the rolling trade buffer.
- `crypto_footprint_response_recorder.py` / `crypto_footprint_response_replay.py`: freeze footprint state beside a quote and compare pressure states with later signed and absolute responses.
- `liquidity_stress_monitor.py`: target-size executable impact + spread + short-horizon EWMA volatility.
- `crypto_liquidity_stress_recorder.py` / `crypto_liquidity_stress_replay.py`: test whether a two-of-three stress state persists across snapshots.
- `crypto_liquidity_stress_response_recorder.py` / `crypto_liquidity_stress_response_replay.py`: freeze stress states beside a quote and compare later BTC movement with watch/normal states.
- `crypto_quarter_hour_flow_replay.py`: tests whether UTC quarter-hour opening taker-flow imbalance aligns with a fixed-horizon perp return.
- `crypto_taker_oi_response_replay.py`: separates taker buy/sell imbalance with rising OI from the same flow with falling OI, then compares fixed-horizon price responses.
- `crypto_oi_price_divergence_response_replay.py`: separates four as-of price/OI quadrants and compares fixed-horizon responses without naming them as long/short positions.
- `crypto_account_ratio_oi_response_replay.py`: separates Bybit holder-count long/short crowding and unwinding states by aligning account-ratio imbalance with OI change and fixed-horizon price responses.

The liquidity-stress case is a risk-context monitor: it asks whether a chosen
notional is expensive to unwind right now, rather than predicting direction.
It requires two of three stress components (impact, spread, volatility) before
reporting `liquidity_stress`; missing depth or candles remains explicit.

The recorder/replay makes this a temporal test instead of treating one stressed
book as a regime. It counts only snapshots with impact, spread and volatility
available, and requires a configurable consecutive run before reporting a
persistent-stress candidate.

The stress-response recorder/replay is a separate fixed-window study. It adds
a synchronized MarketBridge perpetual quote to each stress observation, then
compares signed and absolute BTC movement after `liquidity_stress`,
`liquidity_watch` and `normal_liquidity` snapshots. The response layer does
not claim that expensive execution predicts direction; it exists to test
whether the risk context is followed by a different movement distribution.

Provenance: the decomposition follows the public [Pine Analytics / FlyingTulip
execution-risk discussion on X](https://x.com/PineAnalytics/status/1974474638093590994)
and keeps the displayed-depth caveat aligned with [Binance's public order-book
API documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book).

Provenance: the decomposition follows the public [Pine Analytics / FlyingTulip
execution-aware risk discussion on X](https://x.com/PineAnalytics/status/1974474638093590994),
which emphasizes real order-book depth, target-size slippage and short-horizon
EWMA volatility. The implementation is an independently testable hypothesis,
not an endorsement or a claim that the post's idea is profitable.

`crypto_taker_oi_response_replay.py` adds a native historical taker-volume
input. It does not assume that aggressive flow means informed flow: buy/sell
imbalance with rising OI is labelled new-position pressure, while the same
imbalance with falling OI is labelled possible absorption/closing. Missing
alignment and provider page limits remain explicit.

Provenance: MarketBridge uses Binance's public [Taker Buy/Sell Volume API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Taker-BuySell-Volume),
which documents `takerBuyVol`, `takerSellVol`, value fields, timestamps and
periods from 5m through 1d. This is aggregate provider data, not trader intent,
and the example never places orders.

`crypto_oi_price_divergence_response_replay.py` is the simpler OI/price
decomposition. It aligns each price candle with the latest non-future OI row,
records OI age and provider units, classifies price-up/OI-up, price-up/OI-down,
price-down/OI-up and price-down/OI-down states, then measures later signed and
absolute returns. The labels are observable quadrants only; they do not prove
short covering, new shorts, long liquidation or trader intent.

Provenance: the decomposition is motivated by [TheCryptoData's public OI and
liquidation discussion on X](https://x.com/TheCryptoData/status/1948466627365769584)
and field semantics are checked against [Binance's historical open-interest
documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)
and [OKX's public contract OI documentation](https://www.okx.com/docs-v5/en/#rest-api-trading-data-get-contracts-open-interest-and-volume).

中文：`crypto_taker_oi_response_replay.py` 新增原生历史主动买卖量输入。它不把主动成交直接当成“聪明钱”：
主动买/卖不平衡且 OI 上升标记为新仓压力，同方向不平衡但 OI 下降标记为可能的吸收/平仓，然后比较固定窗口的后续价格响应。
缺失对齐和提供方分页始终保留为证据缺口。

出处：MarketBridge 使用 Binance 公开的[主动买卖量接口](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Taker-BuySell-Volume)，
文档给出 `takerBuyVol`、`takerSellVol`、价值字段、时间戳和 5m 到 1d 周期。数据是提供方聚合，不是交易者意图；示例不下单。

`crypto_account_ratio_oi_response_replay.py` uses Binance global or top-trader
account-share, or Bybit holder-count account-ratio history, to test whether provider-specific
crowding behaves differently when aggregate OI rises or falls. Binance's top
trader accounts and Bybit's holders are distinct populations; neither is a
notional long/short position ratio. The replay keeps provider semantics,
cursor/coverage fields, aligns the ratio with OI and candles, and reports
descriptive fixed-horizon responses only.

The Binance `scope=global` path uses the public all-account ratio,
`scope=top_trader` retains the top-account series, and
`scope=top_trader_position` uses the top-trader position-share endpoint. These
are different populations/measurements and must not be pooled in one statistic.

出处：Bybit [Get Long Short Ratio](https://bybit-exchange.github.io/docs/v5/market/long-short-ratio)
公开 `buyRatio`、`sellRatio`、时间戳和 `nextPageCursor`；Binance [Top Trader Long/Short Account Ratio](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Top-Trader-Long-Short-Ratio)
公开大户账户占比和时间戳；`topLongShortPositionRatio` 另行公开大户仓位占比。不同统计对象和字段不会合并，
不是完整持仓名义金额，也不证明交易者意图；示例只读、只研究、不下单。
Binance 全体账户路径对照官方 [Long/Short Ratio](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Long-Short-Ratio)，同样只保留 provider 语义。

Run / 运行：

```bash
python3 examples/crypto/microstructure/crypto_account_ratio_oi_response_replay.py \
  --symbol BTCUSDT --exchange bybit --period 1h --days 14 \
  --ratio-threshold 0.10 --oi-threshold 0.10 \
  --horizon-bars 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_account_ratio_oi_response_replay.py \
  --symbol BTCUSDT --exchange binance --ratio-scope global --period 1h --days 14 \
  --ratio-threshold 0.10 --oi-threshold 0.10
python3 examples/crypto/microstructure/crypto_account_ratio_oi_response_replay.py \
  --symbol BTCUSDT --exchange binance --ratio-scope top_trader_position --period 1h --days 14 \
  --ratio-threshold 0.10 --oi-threshold 0.10
```

The liquidation-burst replay is deliberately different from the single-event
reversal monitor: it aggregates all public liquidation notional over a rolling
window, compares the next price movement with ordinary candle windows, and
keeps side labels as metadata only. It does not assume that a venue's `sell`
label proves a long liquidation.

The liquidation-burst response recorder/replay is the temporal companion to
that one-page replay. The recorder appends each bounded
`/v1/history/liquidations` response beside a synchronized MarketBridge quote;
the replay deduplicates repeated event rows, reconstructs the rolling notional
at each capture, and compares fixed-record BTC responses after burst versus
ordinary snapshots. A burst is not counted again during the configurable
cooldown, and missing quotes remain outside the aligned sample. This is a
non-directional response study, not a liquidation forecast or order model.

The same recorder also accepts `--source market`. In that mode it reads the
bounded recent event window from `/v1/market/liquidations`, which now retains
distinct live liquidation events per venue/symbol. This is the recommended
path for Binance, Bybit, BitMEX, Gate and other enabled WS feeds; it remains a
live archive with retention and feed-gap limits, not a backfilled ledger.

Provenance: the public [CryptoData liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584)
is an unverified research lead. The event semantics are bounded by
[Binance's public liquidation-order stream documentation](https://developers.binance.com/en/docs/products/derivatives-trading-coin-futures/websocket-market-streams/Liquidation-Order-Streams);
MarketBridge's history endpoint may cover only a provider-limited recent page.

The price-cluster replay is deliberately narrower than a commercial heatmap.
It clusters only observed liquidation prints returned by
`/v1/history/liquidations`; it does not infer untouched liquidation levels,
leverage distributions or a price magnet. A candidate requires a notional
threshold, a minimum share in one price band and a fixed forward window, then
reports an absolute-move comparison rather than a directional trade.

The price-cluster response recorder/replay is the temporal companion to that
single-page analysis. It deduplicates repeated observed liquidation rows,
reconstructs the rolling cluster share at each capture, applies a cooldown,
and compares fixed-record BTC movement after qualifying clusters versus
ordinary windows. The result tests only realized-print concentration; it does
not infer latent liquidation walls, leverage distribution or a price magnet.

Provenance: [CryptoData's public liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584)
is treated as an unverified research lead; the replay tests the threshold and
reports the data-coverage limits instead of repeating the claim.

Provenance for the price-band decomposition: [CoinGlass's public liquidation
heatmap post on X](https://x.com/coinglass_com/status/1930154005491282291) and
the [Glassnode liquidation-heatmap research note](https://research.glassnode.com/liquidation-heatmaps/).
Those sources discuss dense liquidation bands; MarketBridge tests only the
observable executed-print subset and makes the missing latent-level evidence
explicit.

The spot/perp depth-gap monitor is an execution-risk observation motivated by
[a public discussion of the spot/perp depth gap on X](https://x.com/ciaobelindazhou/status/2031929849850273955).
It tests the claim with a target-size snapshot and current basis context; it
does not assume that deeper perp liquidity makes a hedge executable.

The generic microstructure response recorder/replay is separate from the
target-size depth-gap and liquidity-stress cases. It freezes the monitor's
top-level bid/ask imbalance plus funding state beside a perpetual quote, then
compares fixed-record signed and absolute BTC movement after bid pressure, ask
pressure, funding-conflict and balanced-book states. Conflict labels remain
separate instead of being silently promoted to pressure signals.

Provenance: the unverified public [OI/order-flow microstructure discussion on X](https://x.com/xwinfinance/status/2023155692916646257)
motivates the falsifiable pressure/response split, while the normalized fields
are cross-checked against [Binance's official order-book documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book)
and [funding-rate documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).

The ADL-risk monitor is grounded in Binance's first-party [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk),
which describes a high/medium/low symbol-level provider rating updated about
every 30 minutes. It is a liquidation-risk context snapshot, not a forecast,
private account metric or execution signal.

The spot/perp depth-gap response recorder/replay is a separate temporal test.
It adds a perpetual MarketBridge quote to each depth/basis snapshot, then
compares fixed-record signed and absolute BTC movement after
`perp_depth_advantage_observation`, `spot_depth_advantage_observation` and
`no_material_depth_gap`. This does not convert a displayed depth advantage
into a hedge route, arbitrage PnL or execution claim.

Provenance: the public [spot/perp depth-gap discussion on X](https://x.com/ciaobelindazhou/status/2031929849850273955)
is treated as an unverified lead and the book fields are cross-checked against
[Binance's official order-book documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book).

The CVD divergence replay is deliberately separate from breakout confirmation:
it requires a material price move and opposite taker-flow ratio over the same
lookback, then measures the next-window reversal. It only uses public trades on
the requested venue, so it does not represent global flow or a causal signal.

The CVD semantics and single-venue coverage boundary are cross-checked against
[a public CVD indicator explanation](https://mindpillar.com/cvd/); that source
is indicator context, not a performance claim.

The short-squeeze response recorder/replay is the temporal companion to the
compatibility monitor. It reuses the runner's four observable components
(funding, OI change, spot/perp flow and optional liquidation context), preserves
the first-poll OI cold start, and compares score-qualified snapshots with
`observe_only` snapshots at a fixed record-count horizon. Provenance: the
unverified public [L2 imbalance plus funding-extreme perp lead on X](https://x.com/instaclaws/status/2038363051213181035).
This is a response study, not a claim that a squeeze score predicts direction.

The Bollinger case is intentionally separate from the realized-volatility/range
breakout replay: it uses the prior candle's close-only BandWidth quantile and
tests only a subsequent upper/lower-band break. It does not add the public
post's ATR stops, leverage or automated execution. Provenance: the public
[Bollinger BandWidth/Squeeze explanation](https://www.bollingerbands.com/bollinger-band-rules)
and the unverified [VWAP + Bollinger squeeze lead on X](https://x.com/instaclaws/status/2038363051213181035).

The VWAP-deviation case is separate from anchored VWAP reclaim and session
confluence: it resets at UTC midnight, measures a volume-weighted typical-price
VWAP plus weighted standard deviation, and only records a cross-back after the
previous close was outside the configured percentage/standard-deviation band.
Provenance: the public [BTC VWAP mean-reversion study](https://www.coinquant.ai/blog/vwap-strategy-backtest-on-bitcoin-intraday-mean-reversion-results)
and the [VWAP-band construction reference](https://www.basischarts.com/indicators/vwap-bands).
The replay is intentionally able to report a negative result and contains no
position, stop or execution model.

The quarter-hour case is a stricter replay of a public [order-book imbalance and
funding-rate strategy lead on X](https://x.com/instaclaws/status/2038363051213181035),
not an endorsement of its automated-trading claims. It is cross-checked against
the primary [Quarter-Hour Effect research paper](https://arxiv.org/abs/2607.09426),
which studies phase-aligned order flow and later futures returns, and against
[Binance's official funding/order-book documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
MarketBridge only tests the observable single-venue flow/return association;
it does not infer a causal clock effect or provide a timing instruction.

The trade-imbalance-bar replay is a separate event-time case, not another
quarter-hour or candle-window flow test. It closes a bar when cumulative signed
taker notional reaches a caller-fixed threshold (or a hard trade-count cap),
then compares strong buy/sell imbalance bars with balanced controls over the
next event bars. The lead is the public [delta/imbalance-bar discussion on
X](https://x.com/quantbeckman/status/1931965694251253967), cross-checked with
the event-based [crypto microstructure study](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=6057134)
and [Binance's public futures market-data documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
Those sources motivate a falsifiable representation test; they do not establish
that an event bar predicts returns or can be executed after fees and latency.

The VPIN-response replay is a different, non-directional stress case. It uses
fixed quote-notional buckets, computes the rolling mean of absolute signed
imbalance, and compares high-VPIN-proxy buckets with normal buckets over a
later event horizon. The public [Bookmap order-flow/volume discussion on
X](https://x.com/bookmap_pro/status/1945883967409819779) is only a broad
research lead; the VPIN construction is cross-checked with the primary
[crypto microstructure study](https://www.frontiersin.org/journals/blockchain/articles/10.3389/fbloc.2026.1811716/full).
MarketBridge reports a proxy and its coverage, not informed-trader identity,
causality or a kill-switch instruction.

The footprint-response case is the temporal companion to the persistence replay.
It uses the same `/v1/market/footprint` state, joins a MarketBridge quote, and
tests whether bid pressure, ask pressure and ordinary snapshots have different
fixed-record forward distributions. Provenance: the unverified public [OI/flow
confirmation discussion on X](https://x.com/xwinfinance/status/2023155692916646257),
cross-checked against the bounded footprint contract. It does not interpret
footprint bins as resting liquidity, liquidation levels or ownership.

The session-momentum replay turns the existing snapshot filter into a historical
event study. It is motivated by the unverified [15-minute VWAP/EMA/MACD/volume
discussion on X](https://x.com/Gustafssonkotte/status/2030566353178882122) and
uses MarketBridge candle semantics cross-checked against [Binance's official
kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).
The result reports aligned close-to-close returns after a paper hurdle; it is
not a universal session edge or an execution instruction.

The weekly RSI case is a separate fixed-window response test. It computes a
close-only RSI(14) and its 14-week simple average on `/v1/history/candles`
with `interval=1w`, then reports both the future close return and the minimum
path return after each state. The public [Ali Charts RSI discussion on X](https://x.com/ali_charts/status/1952905714957177085)
is treated as an unverified hypothesis lead; the implementation does not claim
the cited 20–30% correction or use it as a trading signal.

The weekday/hour replay turns a recurring-clock claim into a matched control
study. It compares the selected weekday/hour (Tuesday 05:00 UTC by default)
with all other weekdays at that UTC hour, and reports the event candle return,
the next-hour bounce and a later fixed-hour response. Provenance: the
unverified [recurring Tuesday 05:00 UTC BTC-selling discussion on X](https://x.com/Sherlockwhale/status/2041499514033320163),
cross-checked against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).
The matched clock is a descriptive falsification tool; it does not identify an
actor, establish causality or create a timing instruction.

The anchored-VWAP replay is deliberately separate from session VWAP. At each
candle it chooses a low or high only from the preceding lookback, anchors the
typical-price OHLCV VWAP there, and records only a fresh reclaim or rejection
with optional volume confirmation. It then measures the next fixed candle index;
the anchor is not an externally verified news/event timestamp and the result is
not a technical-analysis guarantee. The research lead is the public [anchored
VWAP discussion on X](https://x.com/Jake__Wujastyk/status/1873917626638098894),
cross-checked with [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

The volume-profile case is motivated by the public [compression-to-expansion /
low-volume-node discussion on X](https://x.com/Stoiiic/status/1796078958674628714)
and cross-checked against the research [Liquidity-Driven Breakout Reliability
paper](https://papers.ssrn.com/sol3/Delivery.cfm/5962358.pdf?abstractid=5962358&mirid=1).
Because MarketBridge's historical candle surface does not expose tick-level
volume-at-price, the implementation assigns each candle's volume to its typical
price and labels that approximation explicitly.

The footprint case uses the existing `/v1/market/footprint` surface motivated by
the public [OI/flow confirmation discussion on X](https://x.com/xwinfinance/status/2023155692916646257).
It tests whether a price-bin pressure state persists across snapshots; it does
not claim resting-book liquidity, a liquidation wall, position ownership or a
forward-return edge.

The volatility-breakout replay accepts `--roundtrip-cost-bps` and reports gross
versus cost-adjusted aligned returns. Its candidate verdict requires the
after-cost mean to clear `--min-cost-adjusted-edge-bps` with enough observations;
the hurdle is a transparent sensitivity input, not a venue-specific fill model.
It also forwards candle `coverage_detail` from MarketBridge, so a short provider
page cannot silently look like a complete replay window.

## 中文

这些观察器组合资金费率、OI 变化、现货/永续订单流、盘口深度、价格上下文和清算事件，
输出的是共振证据，不是入场信号。第一次轮询没有 OI 基线是有意设计；不同交易所的清算
side 语义不一定相同，因此回放会保留来源和覆盖元数据，缺失数据降级为 `observe_only`。

ADL response 案例只把 Binance 公布的等级当作 provider 上下文，按高、中、低状态比较
之后 BTC 有符号和绝对波动；不声称发生了 ADL，也不把等级当成价格预测。Binance 说明该等级
会综合保险基金、仓位集中、深度、波动率、杠杆和保证金利用率，私有账户风险仍不在本项目内。
研究线索参考 [X 上的公开清算讨论](https://x.com/angustias87/status/2039147109228925373)
和 [Binance ADL 风险 API 文档](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)。

案例包括：

- `short_squeeze_monitor.py`：负资金费率 + OI 上升 + 现货/永续订单流背离。
- `crypto_short_squeeze_response_recorder.py` / `crypto_short_squeeze_response_replay.py`：把四项共振分数与报价一起冻结，比较达到分数门槛与 `observe_only` 快照之后的固定记录窗口 BTC 响应。
- `exhaustion_short_monitor.py`：正资金费率 + 冲高失败 + OI 下降 + 买盘变弱。
- `liquidation_reversal_monitor.py`：卖方清算 + OI 下降 + CVD 转正 + 价格恢复。
- `crypto_liquidation_burst_replay.py`：滚动清算名义金额阈值与未来绝对价格波动对比。
- `crypto_liquidation_burst_response_recorder.py` / `crypto_liquidation_burst_response_replay.py`：把滚动 burst 与同步报价冻结到 JSONL，去重重复事件后比较 burst 与普通快照的后续 BTC 响应。
- `crypto_liquidation_price_cluster_replay.py`：把已观测清算成交按价格带聚类，并与普通窗口的未来绝对波动比较。
- `crypto_liquidation_price_cluster_response_recorder.py` / `crypto_liquidation_price_cluster_response_replay.py`：把已观测价格带集中状态与报价冻结，比较之后固定窗口的 BTC 波动。
- `crypto_microstructure_monitor.py`：盘口失衡结合资金费率上下文。
- `crypto_microstructure_response_recorder.py` / `crypto_microstructure_response_replay.py`：把盘口失衡/资金费率状态与同步报价冻结，比较压力、冲突和普通状态之后的 BTC 响应。
- `crypto_flow_book_confirmation.py`：订单流确认或否定 L2 压力。
- `crypto_cvd_divergence_replay.py`：检验单交易所价格与主动买卖差值背离后，固定窗口是否反转。
- `crypto_trade_imbalance_bar_replay.py`：按固定名义金额阈值（或交易笔数上限）构造事件条，比较强主动买卖不平衡与普通平衡事件条之后的事件时间收益。
- `crypto_vpin_response_replay.py`：按固定成交量桶计算滚动绝对主动买卖不平衡均值，比较高 VPIN 代理状态与普通状态之后的绝对波动。
- `crypto_derivatives_sentiment_monitor.py`：读取可选 CoinGlass 的资金费率、OI、long/short 与清算上下文，不把聚合指标解释成持仓归属。
- `crypto_adl_risk_monitor.py`：读取 Binance symbol-level 高/中/低 ADL 风险等级，只作为清算风险上下文，不作为方向信号。
- `crypto_derivatives_sentiment_recorder.py` / `crypto_derivatives_sentiment_replay.py`：记录 CoinGlass 聚合情绪并要求连续拥挤状态后才报告持续性。
- `crypto_derivatives_crowding_response_recorder.py` / `crypto_derivatives_crowding_response_replay.py`：把同一聚合上下文和价格快照一起冻结，比较多头/空头拥挤后的固定记录窗口签名收益，并单独统计伴随清算的样本。
- `crypto_spot_perp_depth_gap_monitor.py`：比较同交易所现货/永续的目标规模深度与冲击。
- `crypto_spot_perp_depth_gap_recorder.py` / `crypto_spot_perp_depth_gap_replay.py`：检验该深度差是否在多个快照中持续。
- `crypto_spot_perp_depth_gap_response_recorder.py` / `crypto_spot_perp_depth_gap_response_replay.py`：把深度差状态与同步报价冻结，按永续优势、现货优势和无明显差异比较后续 BTC 响应。
- `crypto_volatility_breakout_replay.py`：压缩区间突破结合成交量确认，并测量未来收益。
- `crypto_bollinger_squeeze_replay.py`：用前一根 K 线的 BandWidth 历史分位识别压缩，再检验上下轨突破后的固定窗口延续。
- `crypto_vwap_deviation_reversion_replay.py`：按 UTC 日重置 VWAP，用成交量加权典型价标准差识别前一根偏离，再检验回穿 VWAP 后的固定窗口方向响应。
- `crypto_session_filter.py`：VWAP/EMA/MACD/成交量的时段过滤回放。
- `crypto_weekday_hour_effect_replay.py`：把指定 UTC 星期/小时与其他星期同一小时做匹配时钟对照，检验事件、反弹与后续响应。
- `crypto_anchored_vwap_replay.py`：以前置窗口 swing low/high 为锚点，检验 VWAP 夺回/跌破后的固定窗口响应。
- `liquidity_stress_monitor.py`：目标名义金额的可执行冲击 + 点差 + 短周期 EWMA 波动率。
- `crypto_liquidity_stress_recorder.py` / `crypto_liquidity_stress_replay.py`：检验两项以上压力条件是否在多个快照中持续。
- `crypto_liquidity_stress_response_recorder.py` / `crypto_liquidity_stress_response_replay.py`：把压力状态与同步行情冻结，比较 `liquidity_stress`、`liquidity_watch` 和普通状态之后的 BTC 响应。

流动性压力案例是风险上下文观察器：它回答“现在以指定名义金额退出是否昂贵”，
而不是预测涨跌。冲击、点差、波动率三项中至少两项达到阈值才报告
`liquidity_stress`；盘口或 K 线缺失会明确保留，不会填成零。

recorder/replay 把它变成时间维度的检验，避免把一个压力盘口误当成持续状态。只有冲击、点差和
波动率都可用的快照进入有效覆盖率；连续达到可配置 run 后才报告持续压力候选。

stress-response recorder/replay 是单独的固定窗口研究：为每个压力快照补充同步的 MarketBridge 永续报价，
再比较 `liquidity_stress`、`liquidity_watch` 和 `normal_liquidity` 之后的有符号/绝对 BTC 波动。
它不声称昂贵的成交环境可以预测方向，只检验风险上下文后面的收益分布是否不同。

出处：拆解自公开 [Pine Analytics / FlyingTulip 的执行风险讨论](https://x.com/PineAnalytics/status/1974474638093590994)，
并参考 [Binance 官方盘口 API 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book)，
同时保留“展示深度不等于可成交容量”的限制。

出处：实现拆解自 [Pine Analytics / FlyingTulip 在 X 的执行风险讨论](https://x.com/PineAnalytics/status/1974474638093590994)，
原文强调真实盘口深度、目标规模滑点和短周期 EWMA 波动率。这里是独立、可证伪的
研究假设，不代表对原文或盈利能力的背书。

清算 burst 回放与单次事件反转监控不同：它在滚动窗口内聚合所有公开清算名义金额，
再和普通 K 线窗口的未来价格波动比较；side 只作为元数据保留，不假设交易所的
`sell` 一定代表多头清算。

清算 burst response recorder/replay 是上述单页回放的时间维度 companion：recorder 把每次
`/v1/history/liquidations` 的有界响应与同步 MarketBridge 报价追加到 JSONL；replay 只按可观察字段去重跨快照重复事件，
在每个采集点重建滚动名义金额，并比较 burst 与普通快照之后固定记录窗口的 BTC 响应。可配置 cooldown，避免同一 burst
被连续快照重复计数；报价缺失会排除出对齐样本。这是非方向性的响应研究，不是清算预测或下单模型。

出处：公开 [CryptoData 在 X 的清算阈值讨论](https://x.com/TheCryptoData/status/1948466627365769584)
只是未经验证的研究线索；事件语义以 [Binance 官方清算订单流文档](https://developers.binance.com/en/docs/products/derivatives-trading-coin-futures/websocket-market-streams/Liquidation-Order-Streams)
为边界，MarketBridge 的历史接口仍可能只覆盖提供方最近的一页数据。

价格带聚类回放比商业热图更窄：它只聚类 `/v1/history/liquidations` 返回的已发生清算成交，
不推断尚未触发的清算价、杠杆分布或“价格磁铁”。必须同时满足名义金额阈值、单一价格带占比
阈值和固定未来窗口，输出仍是绝对波动比较，不是方向性交易。

价格带 response recorder/replay 是上述单页分析的时间维度 companion：按可观察的时间、价格、名义金额和 side 去重重复行，
在每个采集点重建滚动 cluster share，应用 cooldown，再比较符合条件的价格带与普通窗口之后固定记录数的 BTC 波动。
它只检验已发生清算成交的集中度，不推断潜在清算墙、杠杆分布或价格磁铁。

出处：[CryptoData 在 X 的清算阈值讨论](https://x.com/TheCryptoData/status/1948466627365769584)
只是未经验证的研究线索；回放会检验阈值，并把覆盖范围限制明确输出，而不是复述结论。

价格带拆解出处：[CoinGlass 在 X 的公开清算热图帖子](https://x.com/coinglass_com/status/1930154005491282291)
以及 [Glassnode 清算热图研究说明](https://research.glassnode.com/liquidation-heatmaps/)。这些资料讨论密集清算带；
MarketBridge 只验证已观测成交子集，并明确潜在清算墙数据缺失。

现货/永续深度差监控的研究线索来自[公开 X 讨论](https://x.com/ciaobelindazhou/status/2031929849850273955)。
它用目标规模盘口和当前 basis 做执行风险观察，不假设永续深度更深就代表对冲一定可成交。

generic microstructure response recorder/replay 与目标规模深度差和流动性压力案例分开：它冻结 monitor 的 top-level bid/ask
失衡、资金费率状态和永续报价，再比较固定记录窗口中 bid pressure、ask pressure、资金费率冲突和 balanced-book 状态的
有符号/绝对 BTC 波动。冲突标签保持独立，不会静默升级成压力信号。

出处：未经验证的 [X 上 OI/订单流微结构讨论](https://x.com/xwinfinance/status/2023155692916646257)
只用于提出压力/响应的可证伪拆分；盘口字段对照 [Binance 官方 order-book 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book)，
资金费率字段对照 [官方 funding-rate 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)。

ADL 风险监控依据 Binance 官方 [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)，
文档说明它是约每 30 分钟更新的 symbol-level 高/中/低提供方等级。这里仅作为清算风险上下文，
不是价格预测、私人账户风险或执行信号。

spot/perp depth-gap response recorder/replay 是独立的时间检验：为每个深度/basis 快照补充永续 MarketBridge 报价，
再按 `perp_depth_advantage_observation`、`spot_depth_advantage_observation` 和 `no_material_depth_gap` 比较固定记录窗口的
BTC 有符号/绝对波动。这不会把展示深度优势转成对冲路由、套利 PnL 或执行结论。

出处：公开 [现货/永续深度差 X 讨论](https://x.com/ciaobelindazhou/status/2031929849850273955)
只是未经验证的研究线索，盘口字段对照 [Binance 官方 order-book 文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book)。

波动率突破回放支持 `--roundtrip-cost-bps`，同时输出 gross 与扣除纸面成本后的方向收益、命中率和 verdict；
候选必须满足 after-cost 平均 edge 与最小样本数。这个门槛是透明敏感性输入，不是交易所成交模型。

`crypto_quarter_hour_flow_replay.py` 是对公开“订单簿不平衡 + 资金费率”线索的更严格回放：取 UTC 每 15 分钟开盘后
的短窗口主动买卖差值，检验其是否与固定未来永续收益方向一致。它不是自动交易策略，也不把时钟阶段当作因果因素；
公开成交历史的覆盖、交易所 side 语义、手续费、排队、延迟和成交都保持为缺口。

出处：公开 [X 上的订单簿不平衡/资金费率策略线索](https://x.com/instaclaws/status/2038363051213181035)
只作为未经验证的研究线索；实现对照一级研究 [Quarter-Hour Effect](https://arxiv.org/abs/2607.09426)，
并对照 [Binance 官方资金费率与盘口文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)。
MarketBridge 只检验单交易所可观测的流量与收益关联，不提供择时指令。

`crypto_trade_imbalance_bar_replay.py` 是独立的事件时间案例，不重复 15 分钟或 K 线窗口流量检验：累计带符号的主动成交名义金额
达到调用者阈值（或交易笔数上限）才关闭事件条，再把强买/强卖不平衡与平衡控制条的后续事件条收益做对照。研究线索来自公开
[X 上的 delta/imbalance-bars 讨论](https://x.com/quantbeckman/status/1931965694251253967)，并对照
[加密市场事件型微结构研究](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=6057134)
和 [Binance 公共 futures 市场数据文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)。
这些资料只支持可证伪的数据表示测试，不证明事件条能预测收益，也不证明扣除手续费和延迟后可执行。

VPIN response 是另一个非方向性压力案例：用固定名义金额成交量桶计算滚动绝对带符号不平衡均值，
再比较高 VPIN 代理状态与普通状态在后续事件时间窗口的绝对波动。公开的
[Bookmap 订单流/大成交量讨论](https://x.com/bookmap_pro/status/1945883967409819779) 只是宽泛研究线索；
VPIN 构造对照一级的[加密微结构研究](https://www.frontiersin.org/journals/blockchain/articles/10.3389/fbloc.2026.1811716/full)。
MarketBridge 输出的是代理指标和覆盖信息，不识别知情交易者、不证明因果，也不提供 kill-switch 指令。

`crypto_session_momentum_replay.py` 把已有的当前快照筛选升级成历史事件研究：在调用者指定的本地时段内，计算
session VWAP、EMA(9/21)、MACD 加成交量确认，并测量固定未来 K 线窗口的方向收益。研究线索来自未经验证的
[X 上 15 分钟 VWAP/EMA/MACD/成交量讨论](https://x.com/Gustafssonkotte/status/2030566353178882122)，
K 线语义对照 [Binance 官方文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。
输出是纸面 close-to-close 统计，不是普适时段优势或执行指令。

`crypto_weekday_hour_effect_replay.py` 把周期性时钟说法拆成匹配对照：默认比较周二 05:00 UTC 与其他星期同一小时，
分别输出事件 K 线、下一小时反弹和固定小时后的响应。出处是未经验证的
[X 上“周二 05:00 UTC BTC 卖压”讨论](https://x.com/Sherlockwhale/status/2041499514033320163)，并对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

`crypto_atr_regime_response_replay.py` 另行研究波动率状态：用简单平均真实波幅（ATR）和不包含当前值的滚动分位数，将状态分为压缩、普通和扩张，比较之后的有符号收益、绝对收益和路径风险。
它不预测方向、不计算仓位、不设置止损，也不执行交易。研究线索来自
[X 上的 regime/ATR 讨论](https://x.com/viviennaBTC/status/2037854988442235187)，计算口径对照
[Binance Academy ATR 说明](https://www.binance.com/en/square/post/510812)。

`crypto_breakout_retest_response_replay.py` 研究与 sweep 相反的延续路径：收盘突破前序回看区间后，在限定窗口内触及被突破水平，并重新收在突破方向一侧；响应从回踩收盘开始。
它不把 OHLCV 水平解释为真实支撑/阻力、挂单或成交保证。研究线索来自
[Rekt Capital 在 X 的 BTC 突破/回踩讨论](https://x.com/rektcapital/status/1850982324621676715)，并对照
[Binance Academy 的加密突破说明](https://www.binance.com/en/academy/articles/a-beginners-guide-to-swing-trading-cryptocurrency)。

`crypto_ichimoku_cloud_response_replay.py` 研究 point-in-time Ichimoku 状态：价格相对云层位置、Tenkan/Kijun、云颜色和 Chikou 比较共同分组；当前云层严格使用位移以前的历史计算，不把未来投影泄漏到当前特征。
它只是响应分布研究，不是预测或执行规则。研究线索来自未经验证的
[X 上 Ichimoku/云层讨论](https://x.com/Invst_Informant/status/2014788740992929906)，公式对照
[Binance Academy Ichimoku 说明](https://www.binance.com/en/academy/articles/ichimoku-clouds-explained)。

`crypto_rsi_bollinger_extreme_response_replay.py` 把 RSI 和 Bollinger 的组合极值单独拆出：RSI-only、Bollinger-only、共同超买/超卖和 ordinary 四类，比较各自未来响应。
它不把极值当成必然反转，也不生成下单规则。研究线索来自
[X 上 BTC RSI + 上轨讨论](https://x.com/MichaelMOTTCM/status/1944846581611814956)，定义对照
[Binance RSI 词典](https://www.binance.com/en/academy/glossary/relative-strength-index)
和 [Bollinger Bands 说明](https://www.binance.com/en/square/post/42841)。

`crypto_fibonacci_retracement_response_replay.py` 把 Fibonacci 回撤拆成可证伪的 OHLCV 响应研究：
只用当前 K 线以前的回看窗口选 swing high/low，按高低点时间顺序计算方向，再比较 38.2%、50%、61.8% 附近、
区间内其他位置和区间外的固定窗口响应。它是动态回看锚点的代理，不是主观画线、支撑/阻力保证或执行规则。
研究线索来自 [X 上 WIF Fibonacci 讨论](https://x.com/CryptoJournaal/status/2026693734063075685)，
定义对照 [Binance Academy Fibonacci 指南](https://www.binance.com/en/academy/articles/a-guide-to-mastering-fibonacci-retracement)
和 [Binance 词典](https://www.binance.com/en/academy/glossary/fibonacci-retracement)。
最高点和最低点若落在同一根 K 线，则保留为 `missing_swing`，不人为补方向。

`crypto_fair_value_gap_response_replay.py` 研究三根 K 线的 wick 不重叠区域：中间 K 线实体必须达到给定比例，
当前 K 线形成 bullish 或 bearish zone 后，后续窗口分别记录 untouched、touched 和 wick_filled，
并与普通 K 线比较未来响应。这里的 gap 是 OHLCV 代理，不是未成交量或机构意图；研究线索来自
[X 上的 FVG 教学讨论](https://x.com/Bradgohtrades/status/2058684241958031814)，字段对照
[Binance Academy 的蜡烛图说明](https://www.binance.com/en/square/post/492082)
和 [Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

`crypto_obv_divergence_response_replay.py` 与 CVD 不同：它按收盘方向给整根 K 线成交量加减，
将 OBV 变化除以回看窗口总成交量，比较价格/OBV 背离、同向确认和 mixed 状态的后续响应。
它不观察主动成交方向、持仓归属或鲸鱼意图；缺失 volume 保留为 `missing_volume`，不会填零。
定义对照 [Binance 官方 OBV 说明](https://www.binance.com/en/square/post/1218711) 和
[Fidelity 的 OBV 公式与限制](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/OBV)。

```bash
python3 examples/crypto/microstructure/crypto_fibonacci_retracement_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 90 --level-tolerance 0.03 \
  --horizon-bars 6 --min-observations 5
```

```bash
python3 examples/crypto/microstructure/crypto_obv_divergence_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 20 --price-threshold-bps 20 \
  --obv-threshold 0.10 --horizon-bars 6 --min-observations 5
```

匹配时钟只是证伪工具，不识别行为主体、不证明因果，也不生成择时指令。

`crypto_anchored_vwap_replay.py` 与 session VWAP 分开：每根 K 线只从前置回看窗口选择 swing low 或 swing high，
以该点开始计算 typical-price OHLCV VWAP，只记录新的夺回或跌破，并可要求成交量确认，再测量固定未来 K 线窗口。
锚点不是外部验证的新闻/事件时间戳，结果也不是技术分析保证。研究线索来自公开的
[anchored VWAP X 讨论](https://x.com/Jake__Wujastyk/status/1873917626638098894)，K 线语义对照
[Binance 官方文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

`crypto_volume_profile_breakout_replay.py` 检验低成交量节点（LVN）突破：用历史 OHLCV 近似 volume-at-price，
从滚动 profile 计算 value area，要求价格离开 value area、进入低量 bin 并有成交量确认，再测量固定未来窗口。
研究线索来自公开 [X 上的压缩到扩张/低量节点讨论](https://x.com/Stoiiic/status/1796078958674628714)，并对照
[Liquidity-Driven Breakout Reliability 研究](https://papers.ssrn.com/sol3/Delivery.cfm/5962358.pdf?abstractid=5962358&mirid=1)。
由于接口没有 tick 级 volume-at-price，这里明确把每根 K 线成交量分配到 typical price，只是近似，不是订单簿事实。

`crypto_liquidity_sweep_response_replay.py` 只检验公开“扫损/收回”叙事中能从 OHLCV 看见的部分：当前 K 线刺破前序回看区间的高点或低点，收盘重新穿回该水平，且实体/波动达到阈值，随后比较方向对齐的固定窗口收益。
它不声称看到了真实止损池、挂单流动性、CISD 或 displacement 意图。研究线索来自
[KM Trading 在 X 的 setup 拆解](https://x.com/KMTrading_SMC/status/2032428981040103847)，字段语义对照
[Binance 官方 K 线文档](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)。

`crypto_footprint_imbalance_monitor.py` 使用已有 `/v1/market/footprint`，读取价格分桶的 bid/ask delta 和 stacked
imbalance，并由 recorder/replay 检验压力状态是否连续出现。研究线索参考公开的
[OI/订单流确认讨论](https://x.com/xwinfinance/status/2023155692916646257)；这里不把它解释成挂单流动性、清算墙、
持仓归属或未来收益优势。

footprint response 案例是持续性回放的时间 companion：使用相同的 `/v1/market/footprint` 状态，
和 MarketBridge 行情配对，检验 bid pressure、ask pressure 与普通状态在固定记录窗口后的有符号和绝对响应是否不同。
出处仍是未经验证的 [X 上 OI/订单流确认讨论](https://x.com/xwinfinance/status/2023155692916646257)；不把分桶压力解释成挂单流动性、清算价位或持仓归属。

CVD 背离回放与突破确认不同：它要求同一回看窗口内价格有足够幅度、但单交易所主动买卖差值指向相反，
再测量未来窗口是否反向移动。它不代表全市场流量，也不是因果信号；指标语义和单交易所覆盖边界可对照
[CVD 说明](https://mindpillar.com/cvd/)。

short-squeeze response recorder/replay 是兼容性监控的时间维度 companion：复用资金费率、OI 变化、现货/永续
流量和可选清算上下文四项可观测证据，保留第一次轮询没有 OI 基线的冷启动，并在固定记录数窗口比较达到分数门槛
与 `observe_only` 快照之后的 BTC 响应。出处是未经验证的[公开 X 上 L2 不平衡与极端资金费率线索](https://x.com/instaclaws/status/2038363051213181035)。
它是响应研究，不是对逼空方向预测能力的声明。

Bollinger 案例与已有 realized-volatility/区间突破回放分开：它只用前置 K 线的 close 计算 BandWidth 历史分位，
再检验随后突破上轨或下轨后的固定窗口延续，不加入公开帖子里的 ATR 止损、杠杆或自动执行。出处是公开的
[Bollinger BandWidth/Squeeze 说明](https://www.bollingerbands.com/bollinger-band-rules)，以及未经验证的
[X 上 VWAP + Bollinger squeeze 线索](https://x.com/instaclaws/status/2038363051213181035)。

VWAP deviation 案例与 anchored VWAP 夺回和 session confluence 分开：按 UTC 午夜重置，用成交量加权典型价计算 VWAP 及
标准差，只记录前一根收盘价在百分比/标准差带外、当前收盘回穿 VWAP 的事件。出处参考公开的
[BTC VWAP 均值回归研究](https://www.coinquant.ai/blog/vwap-strategy-backtest-on-bitcoin-intraday-mean-reversion-results)
和 [VWAP band 计算说明](https://www.basischarts.com/indicators/vwap-bands)。回放可以如实报告负结果，
不含仓位、止损或执行模型。

`crypto_derivatives_sentiment_monitor.py` 使用可选的 CoinGlass aggregate signal，把资金费率、OI、
long/short ratio、basis 和 liquidation 放在同一上下文中；API key 缺失或指标缺失会保持为 observe-only，
不会把 aggregate ratio 解释成真实持仓归属。

The sentiment recorder/replay turns that snapshot into a falsifiable temporal
check: it appends the aggregate state to JSONL, sorts by capture time, and only
reports a persistent long/short crowding candidate after `--min-run` consecutive
states. A provider ratio is not position ownership; the replay has no price,
funding-income, allocation or execution model.

The crowding-response recorder/replay is a separate fixed-window study. It
freezes the aggregate context beside a MarketBridge quote, scores long crowding
as the negative of the next return and short crowding as the positive return,
and keeps liquidation-qualified observations in separate buckets. This signed
transform is descriptive rather than a directional recommendation. The research
lead is [CryptoData's public liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584).

情绪 recorder/replay 把单次快照变成可证伪的时间检验：将聚合状态追加到 JSONL，按采集时间排序，
只有连续 `--min-run` 次多头/空头拥挤状态才报告持续候选。提供方 ratio 不是持仓归属；回放不含价格、
资金费收入、资金分配或执行模型。

拥挤响应 recorder/replay 与上述持续性回放不同：它把“拥挤一侧叠加大量清算后可能出现反转”拆成固定记录窗口的
签名响应研究。多头拥挤取下一段收益的负值，空头拥挤取正值，并单独保留伴随清算的样本；这只是描述性变换，
不是方向建议。研究线索来自 [CryptoData 在 X 的清算阈值讨论](https://x.com/TheCryptoData/status/1948466627365769584)。

Provenance: the decomposition follows the public [CoinGlass aggregate
positioning discussion on X](https://x.com/ImCryptOpus/status/1949195275903410571),
treated as an unverified lead. MarketBridge tests persistence and exposes
coverage instead of repeating a directional claim.

出处：拆解来自 [CoinGlass 聚合持仓讨论](https://x.com/ImCryptOpus/status/1949195275903410571)，
这里只把它作为未经验证的研究线索；MarketBridge 检验状态持续性并暴露覆盖范围，不复述方向性结论。

## Commands / 命令

```bash
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/exhaustion_short_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/liquidation_reversal_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/liquidity_stress_monitor.py \
  --symbol BTCUSDT --exchange binance --liquidity-target-notional 10000 \
  --liquidity-volatility-bars 60 --iterations 3
python3 examples/crypto/microstructure/crypto_liquidity_stress_recorder.py \
  --symbol BTCUSDT --exchange binance --target-notional 10000 \
  --iterations 60 --interval-secs 30 --output work/crypto-liquidity-stress.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_stress_replay.py \
  --input work/crypto-liquidity-stress.jsonl --min-run 3
python3 examples/crypto/microstructure/crypto_liquidity_stress_response_recorder.py \
  --symbol BTCUSDT --exchange binance --target-notional 10000 \
  --iterations 60 --interval-secs 30 \
  --output work/crypto-liquidity-stress-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_stress_response_replay.py \
  --input work/crypto-liquidity-stress-response.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_liquidation_burst_replay.py \
  --exchange okx --price-exchange okx --symbol BTCUSDT \
  --threshold-notional 1000000 --window-hours 24 --horizon-bars 12
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_recorder.py \
  --exchange okx --price-exchange okx --symbol BTCUSDT \
  --iterations 120 --interval-secs 30 --threshold-notional 1000000 \
  --output work/crypto-liquidation-burst-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_recorder.py \
  --source market --exchange binance --price-exchange binance --symbol BTCUSDT \
  --iterations 120 --interval-secs 30 \
  --output work/crypto-live-liquidation-burst-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_replay.py \
  --input work/crypto-liquidation-burst-response.jsonl \
  --window-hours 24 --horizon-records 12 --threshold-notional 1000000 \
  --cooldown-records 12 --min-observations 3
python3 examples/crypto/microstructure/crypto_liquidation_price_cluster_replay.py \
  --exchange okx --price-exchange okx --symbol BTCUSDT \
  --threshold-notional 1000000 --cluster-band-bps 25 \
  --min-cluster-share 0.5 --window-hours 24 --horizon-bars 12
python3 examples/crypto/microstructure/crypto_liquidation_price_cluster_response_recorder.py \
  --exchange okx --price-exchange okx --symbol BTCUSDT \
  --liquidation-limit 100 --iterations 120 --interval-secs 30 \
  --output work/crypto-liquidation-price-cluster-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_price_cluster_response_replay.py \
  --input work/crypto-liquidation-price-cluster-response.jsonl \
  --window-hours 24 --horizon-records 12 --threshold-notional 1000000 \
  --cluster-band-bps 25 --min-cluster-share 0.5 --cooldown-records 12 \
  --min-observations 3
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_monitor.py \
  --symbol BTCUSDT --exchange binance --target-notional 10000 \
  --min-depth-ratio 2.0 --min-impact-improvement-bps 5
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-spot-perp-depth.jsonl
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_replay.py \
  --input work/crypto-spot-perp-depth.jsonl --min-depth-ratio 2.0 --min-run 3
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_response_recorder.py \
  --symbol BTCUSDT --exchange binance --target-notional 10000 \
  --iterations 60 --interval-secs 30 \
  --output work/crypto-spot-perp-depth-response.jsonl
python3 examples/crypto/microstructure/crypto_spot_perp_depth_gap_response_replay.py \
  --input work/crypto-spot-perp-depth-response.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_microstructure_response_recorder.py \
  --symbol BTCUSDT --exchange binance --top-levels 5 \
  --iterations 60 --interval-secs 30 \
  --output work/crypto-microstructure-response.jsonl
python3 examples/crypto/microstructure/crypto_microstructure_response_replay.py \
  --input work/crypto-microstructure-response.jsonl \
  --horizon-records 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_volatility_breakout_replay.py \
  --exchange binance --symbol BTCUSDT --interval 5m --days 7 \
  --roundtrip-cost-bps 20 --min-cost-adjusted-edge-bps 0
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_monitor.py \
  --symbol BTC --long-short-high 1.2 --long-short-low 0.8
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_recorder.py \
  --symbol BTC --iterations 20 --interval-secs 30 \
  --output work/crypto-derivatives-sentiment.jsonl
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_replay.py \
  --input work/crypto-derivatives-sentiment.jsonl --min-run 3
python3 examples/crypto/microstructure/crypto_derivatives_crowding_response_recorder.py \
  --symbol BTC --price-symbol BTCUSDT --exchange binance --product-type perp \
  --iterations 30 --interval-secs 30 \
  --output work/crypto-derivatives-crowding-response.jsonl
python3 examples/crypto/microstructure/crypto_derivatives_crowding_response_replay.py \
  --input work/crypto-derivatives-crowding-response.jsonl \
  --horizon-records 7 --min-observations 5 --paper-cost-bps 10
python3 examples/crypto/microstructure/crypto_short_squeeze_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 30 \
  --output work/crypto-short-squeeze-response.jsonl
python3 examples/crypto/microstructure/crypto_short_squeeze_response_replay.py \
  --input work/crypto-short-squeeze-response.jsonl --horizon-records 7 \
  --min-score 3 --paper-cost-bps 10
python3 examples/crypto/microstructure/crypto_bollinger_squeeze_replay.py \
  --exchange binance --symbol BTCUSDT --interval 5m --days 7 \
  --period 20 --deviations 2 --bandwidth-lookback 96 \
  --max-bandwidth-quantile 0.20 --horizon-bars 12 \
  --paper-cost-bps 10 --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_vwap_deviation_reversion_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1h --days 30 \
  --deviation-bps 50 --sigma 2 --horizon-bars 12 \
  --paper-cost-bps 10 --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_session_filter.py \
  --exchange binance --market perp --symbol BTCUSDT --interval 1m --limit 60
python3 examples/crypto/microstructure/crypto_quarter_hour_flow_replay.py \
  --exchange binance --symbol BTCUSDT --days 3 --window-minutes 5 \
  --horizon-bars 240 --min-flow-ratio 0.20 --paper-cost-bps 10 \
  --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_taker_oi_response_replay.py \
  --symbol BTCUSDT --exchange binance --period 5m --days 7 \
  --flow-threshold 0.20 --oi-threshold 0.10 \
  --horizon-bars 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_oi_price_divergence_response_replay.py \
  --symbol BTCUSDT --price-exchange binance --oi-exchange okx \
  --price-interval 5m --oi-interval 5m --days 2 \
  --lookback-bars 3 --horizon-bars 3 --min-observations 5
python3 examples/crypto/microstructure/crypto_trade_imbalance_bar_replay.py \
  --exchange binance --symbol BTCUSDT --days 2 --trade-pages 12 \
  --bar-notional 1000000 --max-trades-per-bar 500 \
  --min-imbalance-ratio 0.60 --horizon-bars 3 \
  --paper-cost-bps 10 --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_vpin_response_replay.py \
  --exchange binance --symbol BTCUSDT --days 2 --trade-pages 12 \
  --bucket-notional 1000000 --window-buckets 20 --horizon-buckets 3 \
  --high-vpin 0.60 --paper-cost-bps 10 --min-edge-bps 0 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_session_momentum_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1m --days 3 \
  --timezone America/New_York --session-start 09:00 --session-end 09:15 \
  --volume-multiplier 1.0 --horizon-bars 15 --min-score 4 \
  --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/microstructure/crypto_weekday_hour_effect_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1h --days 180 \
  --target-weekday 1 --target-hour 5 --bounce-hours 1 --horizon-hours 8 \
  --paper-cost-bps 10 --min-observations 5
python3 examples/crypto/microstructure/crypto_anchored_vwap_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 5m \
  --anchor-lookback 96 --anchor-mode both --horizon-bars 12 \
  --volume-multiplier 1.0 --paper-cost-bps 10 --min-observations 5
python3 examples/crypto/microstructure/crypto_volume_profile_breakout_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1m --days 3 \
  --lookback-bars 120 --bins 24 --value-area-fraction 0.70 \
  --low-volume-quantile 0.25 --breakout-buffer-bps 2 \
  --volume-multiplier 1.2 --horizon-bars 30 \
  --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/microstructure/crypto_liquidity_sweep_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 15m \
  --days 15 --lookback-bars 20 --sweep-buffer-bps 0 \
  --min-body-fraction 0.50 --min-range-bps 5 \
  --horizon-bars 8 --paper-cost-bps 10 --min-observations 5
python3 examples/crypto/microstructure/crypto_atr_regime_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --atr-period 14 --regime-lookback 96 \
  --low-quantile 0.20 --high-quantile 0.80 \
  --horizon-bars 8 --min-observations 5
python3 examples/crypto/microstructure/crypto_breakout_retest_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --lookback-bars 24 --breakout-buffer-bps 2 \
  --retest-window 8 --retest-tolerance-bps 15 --horizon-bars 8 \
  --paper-cost-bps 10 --min-observations 5
python3 examples/crypto/microstructure/crypto_ichimoku_cloud_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --conversion-period 9 --base-period 26 \
  --span-b-period 52 --displacement 26 --horizon-bars 6 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_rsi_bollinger_extreme_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --band-period 20 --deviations 2 \
  --overbought 70 --oversold 30 --horizon-bars 12 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_footprint_imbalance_monitor.py \
  --exchange binance --market perp --symbol BTCUSDT --interval-ms 60000 \
  --scale 1 --imbalance-ratio 3 --stacked-imbalance-range 3 \
  --min-delta-ratio 0.20
python3 examples/crypto/microstructure/crypto_footprint_imbalance_recorder.py \
  --exchange binance --market perp --symbol BTCUSDT --iterations 60 \
  --interval-secs 30 --output work/crypto-footprint-imbalance.jsonl
python3 examples/crypto/microstructure/crypto_footprint_imbalance_replay.py \
  --input work/crypto-footprint-imbalance.jsonl --min-run 3
python3 examples/crypto/microstructure/crypto_footprint_response_recorder.py \
  --exchange binance --market perp --symbol BTCUSDT --iterations 60 \
  --interval-secs 30 --output work/crypto-footprint-response.jsonl
python3 examples/crypto/microstructure/crypto_footprint_response_replay.py \
  --input work/crypto-footprint-response.jsonl --horizon-records 3 \
  --min-observations 5
python3 examples/liquidation_reversal_replay.py \
  --exchange coinex --price-exchange binance --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000
```
