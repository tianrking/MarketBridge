# MarketBridge strategy demo library

The maintained crypto entrypoints are grouped under [`crypto/`](crypto/README.md)
with bilingual (English/中文) guides for carry, DeFi, macro, microstructure,
on-chain, options, sentiment and universe research. The root scripts remain compatibility entrypoints and shared
Python implementations; there are no Rust strategy examples in `examples/`.

维护中的加密策略入口统一放在 [`crypto/`](crypto/README.md)，每个系列目录都有中英文
说明，覆盖套利/资金费率、DeFi、宏观、微结构/清算、链上、期权/波动率、情绪和资产宇宙。根目录脚本保留为兼容
入口和共享 Python 实现；`examples/` 中不再放 Rust 策略示例。

These examples are research observers. They call the running MarketBridge HTTP
API, print evidence and a bounded hypothesis score, and never place orders.
Each demo must state its data assumptions and keep missing data visible.

Strategy code is Python-first. Rust remains the framework/runtime layer for
connectors, normalization, caches, history, replay primitives and API serving.
New strategy experiments should start from the relevant categorized Python
launcher or `python_strategy_runner.py`; Rust remains the data/runtime layer.
The `crypto/` paths are canonical for new users. Unqualified `crypto_*.py`
rows below are retained compatibility entrypoints to the same Python
implementations, not separate strategies; when both forms appear, the
categorized command is the recommended one.

新用户应优先使用 `crypto/` 下的分类入口。下表中不带目录的 `crypto_*.py` 是保留的兼容入口，
与分类入口共享同一份 Python 实现，不代表另一套策略；同时出现两种路径时，以分类命令为准。

## Current cases

| Demo | Strategy hypothesis | MarketBridge inputs | Status |
|---|---|---|---|
| `crypto/microstructure/short_squeeze_monitor.py` | Negative funding + rising OI + spot/perp flow divergence can identify a squeeze candidate | funding, OI, order flow, liquidations, external liquidation signal | Python research observer |
| `crypto/microstructure/crypto_short_squeeze_response_recorder.py` / `crypto_short_squeeze_response_replay.py` | The short-squeeze confluence score can be compared with a later fixed-record BTC response instead of being treated as a one-shot signal | `/v1/market/funding`, `/v1/market/open-interest`, `/v1/market/order-flow`, `/v1/market/quotes`, JSONL archive | Score-response study; OI cold start, venue semantics, costs and execution remain explicit gaps |
| `crypto/microstructure/exhaustion_short_monitor.py` | Positive funding + failed highs + falling OI + weak bids can identify long exhaustion | funding, OI, klines, order flow, L2, optional on-chain transfers | Python research observer |
| `crypto/carry/basis_carry_monitor.py` | Positive spot/perp basis + positive funding can justify a delta-neutral carry investigation | basis, funding, observed funding interval when available | Python research observer; withholds annualization when interval is unknown |
| `crypto/carry/crypto_basis_recorder.py` / `crypto_basis_replay.py` | An unusually wide same-venue basis may contract over the next fixed snapshot horizon | `/v1/market/basis`, `/v1/market/perpetual-funding` JSONL archive | Descriptive contraction replay; no carry PnL, hedge, borrow or execution model |
| `crypto/microstructure/liquidation_reversal_monitor.py` | Sell-side liquidation + falling OI + positive CVD and price recovery can identify a flush-reversal candidate | liquidations, OI, order flow, klines | Python research observer; liquidation side semantics must be venue-validated |
| `crypto/microstructure/crypto_liquidation_burst_replay.py` | A rolling liquidation-notional burst may precede larger absolute price movement than ordinary windows | `/v1/history/liquidations`, `/v1/history/candles` | Non-directional burst replay; bounded provider history and side semantics remain explicit |
| `crypto/microstructure/crypto_liquidation_burst_response_recorder.py` / `crypto_liquidation_burst_response_replay.py` | A frozen rolling liquidation burst may have a different later BTC response than ordinary snapshots | `/v1/history/liquidations`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; duplicate-event handling, coverage, side semantics and execution remain explicit |
| `crypto/microstructure/crypto_liquidation_price_cluster_replay.py` | A concentrated band of observed liquidation prints may precede larger absolute movement | `/v1/history/liquidations`, `/v1/history/candles` | Observed-print cluster only; no latent liquidation heatmap, direction or execution model |
| `crypto/microstructure/crypto_volatility_breakout_replay.py` | A compressed range break with volume/flow confirmation may continue after a fixed horizon | `/v1/history/candles`, optional `/v1/history/trades` | Gross and optional after-cost replay; no execution or fill model |
| `crypto/microstructure/crypto_bollinger_squeeze_replay.py` | A trailing close-only BandWidth squeeze followed by an upper/lower-band break may continue over a fixed horizon | `/v1/history/candles` | Separate Bollinger response study; parameter sensitivity, bounded candles, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_cvd_divergence_replay.py` | A material price move against single-venue taker-flow delta may be followed by a fixed-horizon reversal | `/v1/history/candles`, `/v1/history/trades` | Bounded CVD divergence replay; venue coverage and direction semantics remain explicit |
| `crypto/microstructure/crypto_quarter_hour_flow_replay.py` | UTC quarter-hour opening taker-flow imbalance may align with a fixed-horizon perp return | `/v1/history/candles` at 1m and `/v1/history/trades` | Phase-aligned single-venue replay; bounded history, clock-phase causality, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_session_momentum_replay.py` | Session VWAP/EMA(9/21)/MACD/volume confluence may align with a fixed-horizon return | `/v1/history/candles` | Timezone-aware close-to-close replay; session definition, missing bars, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_weekday_hour_effect_replay.py` | A selected weekday/hour may have a different event, bounce and later return than other weekdays at the same UTC hour | `/v1/history/candles` | Matched-clock calendar study; weekday selection, missing bars, sample size, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_vwap_deviation_reversion_replay.py` | A prior UTC-session VWAP deviation followed by a cross-back may show directional mean-reversion response | `/v1/history/candles` | OHLCV VWAP-band response study; session reset, parameters, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_anchored_vwap_replay.py` | A reclaim above a prior swing-low anchored VWAP, or rejection below a swing-high anchored VWAP, may align with a fixed-horizon return | `/v1/history/candles` | Prior-window anchor and OHLCV VWAP replay; event identity, tick volume, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_volume_profile_breakout_replay.py` | A close leaving the prior value area into an OHLCV-approximated low-volume node may continue | `/v1/history/candles` | Volume-at-price approximation replay; tick-level profile, thresholds, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_footprint_imbalance_monitor.py` / recorder / replay | Price-bin bid/ask delta and stacked imbalance may persist across rolling trade-buffer snapshots | `/v1/market/footprint` and JSONL archive | Persistence diagnostic; rolling retention, bin semantics, resting liquidity and forward returns remain explicit gaps |
| `crypto/microstructure/crypto_footprint_response_recorder.py` / `crypto_footprint_response_replay.py` | Footprint bid/ask pressure may align with later signed or absolute BTC movement | `/v1/market/footprint`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; rolling buffer, side semantics, causality, costs and execution remain explicit gaps |
| `crypto/microstructure/crypto_derivatives_sentiment_monitor.py` | Aggregate funding/OI/long-short/liquidation context should remain visible without inferring position ownership | `/v1/external/signals?sources=coinglass` | Optional keyed snapshot context; missing metrics remain observe-only and no execution model |
| `crypto/microstructure/crypto_derivatives_sentiment_recorder.py` / `crypto_derivatives_sentiment_replay.py` | Repeated aggregate derivatives crowding states should be tested for persistence rather than promoted from one snapshot | `/v1/external/signals?sources=coinglass`, JSONL archive | Consecutive-state diagnostic; aggregate ratios are not ownership and no price, allocation or execution model |
| `crypto/microstructure/crypto_derivatives_crowding_response_recorder.py` / `crypto_derivatives_crowding_response_replay.py` | Long/short crowding plus optional liquidation activity can be compared with a later fixed-record price response | `/v1/external/signals?sources=coinglass`, `/v1/market/quotes`, JSONL archive | Signed response study; record-count horizon, provider semantics, costs and execution remain explicit gaps |
| `liquidation_reversal_replay.py` | Measure forward price recovery after bounded OKX/CoinEx sell-side liquidation events, optionally joined with public OI | `/v1/history/liquidations`, `/v1/history/candles`, `/v1/history/open-interest` | Partial replay; consumes liquidation `coverage_detail`; CoinEx uses `--price-exchange okx|binance`; historical CVD and execution costs remain explicit gaps |
| `polymarket_complement_monitor.py` | YES ask + NO ask below one can identify a complement-price candidate | Polymarket Gamma metadata, CLOB books | Snapshot candidate only; no fill, fee, latency or resolution replay |
| `polymarket_price_shock_replay.py` | A sharp public probability update may continue over the next few history points | `/polymarket/markets`, `/polymarket/prices-history` | Descriptive continuation replay; the causal evidence timestamp and execution costs remain explicit |
| `polymarket_timing_replay.py` | Early price discovery and late conviction entries may have different resolution-adjusted outcomes | `/polymarket/markets?include_closed=true&order=createdAt`, `/v1/prediction/trades` | Bounded paper replay; earliest observed trade is only a start-time proxy |
| `prediction_trade_flow.py` | Public trade history can be summarized into side flow and replay inputs | `/v1/prediction/trades` | Descriptive trade-flow summary; not a profitability backtest |
| `polymarket_settlement_replay.py` | Buy-under-price-cap entries can be scored against a closed market's resolved outcome | closed Gamma metadata + public trades | Bounded paper replay; no fill completeness or queue model |
| `polymarket_trade_recorder.py` | Bounded pages of public trades can be frozen into a deduplicated research sample | `/v1/prediction/trades` | JSONL observation archive; no private ledger claim |
| `polymarket_calibration_report.py` | Entry prices can be compared with resolved outcomes using calibration bins and Brier/log loss | recorded trades + resolved outcome | Descriptive calibration, not a prediction model |
| `weather_event_observer.py` | A daily weather bucket can be compared with a provider observation/forecast | `/v1/external/weather` | Deterministic observation only; no implied probability |
| `weather_pressure_differential.py` | Compare a weather observation with a matching Polymarket YES ask after an external update | `/v1/external/weather`, `/polymarket/markets`, `/polymarket/books` | Investigation candidate only; identity, probability and fill assumptions remain explicit |
| `weather_market_calibration.py` | Compare archived weather buckets with verified closed-market outcomes and optional YES prices | `/v1/external/weather` + explicit JSONL manifest | Descriptive calibration; identity and resolution rules are caller-owned |
| `crypto/microstructure/crypto_session_filter.py` | Test a short session-window hypothesis with VWAP, EMA(9/21), MACD and volume confirmation | `/v1/market/klines` | Research filter; no universal timing edge or fill model |
| `crypto_volatility_breakout_replay.py` | A range break after compressed realized volatility may continue when candle volume and optional taker flow confirm | `/v1/history/candles`, optional `/v1/history/trades` | Bounded close-to-close replay; no execution, funding or fee model |
| `crypto_options_skew_monitor.py` | Put-wing IV minus call-wing IV and near/far ATM IV term structure expose options hedging demand and volatility regime | `/v1/options/chains` | Snapshot observer using transparent moneyness buckets; no delta-hedge or execution model |
| `crypto_options_skew_recorder.py` | Freeze repeated options skew snapshots so persistence can be tested rather than inferred from one quote | `/v1/options/chains` | Append-only JSONL observation archive; no private ledger or order data |
| `crypto_options_skew_replay.py` | Measure skew persistence and term-state runs from recorded snapshots | explicit JSONL from recorder | Descriptive persistence replay; no option PnL or hedge simulation |
| `crypto/options/crypto_options_skew_response_recorder.py` / `crypto_options_skew_response_replay.py` | Compare later BTC movement after downside-protection, upside-call or balanced wing-IV states | `/v1/options/chains`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; moneyness buckets, expiry roll, option PnL and execution remain explicit gaps |
| `crypto/options/crypto_options_term_structure_replay.py` | Test whether the near/far ATM-IV slope stays in contango or backwardation for a minimum run | JSONL from `crypto_options_skew_recorder.py` | Term-structure persistence diagnostic; expiry roll, quotes, costs and calendar-spread execution remain explicit gaps |
| `crypto/options/crypto_options_term_structure_response_replay.py` | Compare later BTC movement after upward, inverted or flat near/far ATM-IV states | JSONL from `crypto_options_skew_response_recorder.py` | Fixed-record response study; expiry roll, option PnL, hedge and execution remain explicit gaps |
| `crypto/options/crypto_options_bull_call_spread_monitor.py` / recorder / replay | A lower-call ask plus higher-call bid can form a paper debit below strike width for one expiry | `/v1/options/chains`, JSONL archive | Leg-selection and payoff-geometry persistence diagnostic; mark-only quotes, settlement, margin, costs and execution remain explicit gaps |
| `crypto/options/crypto_options_bull_call_spread_response_recorder.py` / `crypto_options_bull_call_spread_response_replay.py` | Observable bull-call-spread quote states may have different later BTC responses than unvalidated snapshots | `/v1/options/chains`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; quote structure is not option PnL, fill, hedge or execution |
| `crypto/defi/crypto_defi_pool_flow_monitor.py` | High swap volume relative to reported DEX-pool liquidity may indicate an execution-pressure regime | `/v1/external/signals?categories=defi_native_state`, `/v1/market/quotes?product_type=dex_pool` | Read-only pool-state monitor; no route, gas, LP PnL or wallet execution |
| `crypto/defi/crypto_stablecoin_depeg_monitor.py` / recorder / replay | Stablecoin quote deviation and spread stress may coincide with larger later absolute BTC movement | `/v1/market/quotes` for selected CEX/DEX pairs and BTCUSDT, plus JSONL archive | Depeg-risk event study; no reserve, redemption, solvency, mean-reversion or execution model |
| `crypto/defi/crypto_stablecoin_rotation_response_replay.py` | A normalized USDC discount/premium versus USDT may align with later BTC direction | JSONL from `crypto_stablecoin_depeg_recorder.py` | Directional response study; one-venue quote, flow causality, conversion, redemption and execution remain explicit gaps |
| `crypto/defi/crypto_defi_pool_flow_recorder.py` / `crypto_defi_pool_flow_replay.py` | Test whether high-turnover or thin-liquidity/high-flow pool states persist across snapshots | JSONL from the DeFi monitor | Persistence diagnostic; provider coverage, on-chain completeness and swap execution remain explicit |
| `crypto/defi/crypto_defi_pool_flow_response_recorder.py` / `crypto_defi_pool_flow_response_replay.py` | Compare later BTC movement after pressure versus ordinary DEX-pool snapshots | `/v1/external/signals?categories=defi_native_state`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; no causal, LP-PnL, route, gas or wallet-execution claim |
| `crypto/onchain/crypto_onchain_transfer_burst_replay.py` | A rolling burst of public large-transfer notional may precede larger absolute price movement | `/v1/onchain/transfers`, `/v1/history/candles` | Non-directional bounded replay; transfer semantics, labels, coverage and execution remain explicit |
| `crypto/onchain/crypto_onchain_transfer_response_recorder.py` / `crypto_onchain_transfer_response_replay.py` | A frozen rolling transfer burst may have a different later BTC response than ordinary windows | `/v1/onchain/transfers`, `/v1/market/quotes`, JSONL archive | Non-directional response study; provider coverage, deduplication, transfer semantics and execution remain explicit |
| `crypto_options_vrp_monitor.py` | Compare selected-expiry ATM mark IV with annualized perp realized volatility | `/v1/options/chains`, `/v1/history/candles` | Snapshot IV-minus-RV observer; maturity, hedge and cost basis stay explicit |
| `crypto/options/crypto_options_vrp_recorder.py` / `crypto_options_vrp_replay.py` | Test whether an IV-minus-RV premium regime persists for one option expiry | `/v1/options/chains`, `/v1/history/candles`, JSONL archive | Descriptive VRP persistence; no option PnL, delta hedge or short-vol execution model |
| `crypto/options/crypto_options_vrp_response_replay.py` | Compare later BTC signed/absolute responses after IV-premium, RV-above-IV and aligned regimes | Reuses VRP JSONL from `/v1/options/chains` and `/v1/history/candles` | Fixed-record surface-response study; expiry roll, IV/RV horizon mismatch and execution remain explicit |
| `crypto_options_gamma_monitor.py` | Map unsigned gamma concentration near spot and dominant strikes without inferring dealer long/short gamma | `/v1/options/chains` plus bounded `/options/deribit/book` enrichment | Snapshot gamma map; relative mass only, partial coverage is reported, not USD exposure or a directional signal |
| `crypto_options_gamma_recorder.py` / `crypto_options_gamma_replay.py` | Test whether unsigned near-spot gamma concentration persists across snapshots | `/v1/options/chains` JSONL archive | Descriptive persistence replay; no dealer sign, realized-volatility response or hedge PnL |
| `crypto/options/crypto_options_gamma_response_recorder.py` / `crypto_options_gamma_response_replay.py` | Compare later BTC absolute and signed movement after unsigned near-spot gamma concentration versus other snapshots | `/v1/options/chains`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; no dealer sign, option PnL, hedge, causality or execution |
| `crypto_universe_opportunity_scan.py` | Rank a bounded perp universe by stored-kline liquidity/realized volatility plus current funding magnitude | `/v1/universe/top-volume`, `/v1/universe/volatility`, `/v1/market/perpetual-funding` | Candidate discovery only; missing joins and unknown funding intervals remain explicit |
| `crypto/universe/crypto_universe_opportunity_recorder.py` / `crypto_universe_opportunity_replay.py` | Test whether top-k universe candidates persist across snapshots | JSONL from universe and funding endpoints | Persistence diagnostic; no allocation, sizing or execution model |
| `crypto/universe/crypto_universe_opportunity_response_recorder.py` / `crypto_universe_opportunity_response_replay.py` | Compare the next equal-weight top-k candidate basket with BTC after point-in-time universe ranking | `/v1/universe/top-volume`, `/v1/universe/volatility`, `/v1/market/perpetual-funding`, `/v1/market/quotes`, JSONL archive | Candidate-response paper index; missing prices, turnover, funding and execution remain explicit |
| `crypto_cross_asset_momentum_replay.py` | Test whether the strongest trailing BTC/ETH/SOL (or caller-selected) assets beat an equal-weight basket over the next fixed horizon | `/v1/history/candles` for each symbol, exact timestamp intersection | Gross close-to-close replay; no fees, funding, slippage, weight drift or execution model |
| `crypto/universe/crypto_altcoin_breadth_replay.py` | The fraction of selected altcoins beating BTC over a trailing window may separate the next altcoin-basket-versus-BTC relative response | `/v1/history/candles` for BTC and caller-selected altcoins, exact timestamp intersection | Equal-count breadth proxy; not the official market-cap Top-50 index, no allocation or execution model |
| `crypto_volatility_adjusted_momentum_replay.py` | Test whether trailing return divided by per-bar realized volatility improves cross-asset ranking versus an equal-weight basket | `/v1/history/candles` for each symbol, exact timestamp intersection | Risk-adjusted ranking replay; optional fixed paper cost hurdle, no allocation/fill model |
| `crypto/universe/crypto_adaptive_cross_asset_replay.py` | Volatility-normalized signed signals may reduce net exposure when BTC/ETH/SOL signals conflict, compared with an equal-weight basket | `/v1/history/candles` for each symbol, exact timestamp intersection | Adaptive paper-index study; neutral threshold, costs, funding, turnover and execution remain explicit gaps |
| `crypto_volatility_adjusted_momentum_sweep.py` | Expose sensitivity across lookback, volatility and forward-horizon windows without selecting a live parameter | `/v1/history/candles` fetched once per symbol, bounded parameter grid | In-sample diagnostic with optional paper cost hurdle; best row requires time-held-out validation |
| `crypto_volatility_adjusted_momentum_walkforward.py` | Evaluate one fixed risk-adjusted momentum parameter set on a later chronological holdout | `/v1/history/candles` for each symbol, exact timestamp intersection | Holdout paper diagnostic; one split is not proof of stable alpha |
| `crypto/universe/crypto_pairs_mean_reversion_replay.py` | An extreme two-asset log-price spread may shrink toward its frozen trailing mean over a fixed horizon | `/v1/history/candles` for two selected symbols, exact timestamp intersection | Relative-price convergence diagnostic; fixed hedge ratio, costs and paired execution remain explicit gaps |
| `crypto/universe/crypto_universe_delist_risk_monitor.py` | Missing or stale current quotes should be reviewed before treating a historical market as a research candidate | `/v1/universe/delist-risk` | Data-quality guard only; not a delisting forecast and no automatic exclusion |
| `crypto/universe/crypto_market_regime_monitor.py` | Aggregate fragmentation, volatility and leverage context should remain visible before a strategy case is interpreted | `/v1/research/market-regime` | Context monitor only; current snapshot, not historical point-in-time data, and no strategy selection |
| `crypto/universe/crypto_market_regime_recorder.py` / `crypto_market_regime_replay.py` | Fragmented, high-volatility, leveraged and normal aggregate states can be compared with later BTC response distributions | `/v1/research/market-regime`, `/v1/market/quotes`, JSONL archive | Context-response study; current-feature freshness, causality, costs and execution remain explicit gaps |
| `crypto/macro/crypto_macro_context_monitor.py` | Macro reference snapshots should remain visible beside crypto funding before interpreting a market case | `/v1/market/quotes?exchanges=dxy,vix,us10y`, `/v1/market/perpetual-funding` | Context monitor only; no macro forecast or execution model |
| `crypto/macro/crypto_macro_context_recorder.py` / `crypto_macro_context_replay.py` | Elevated VIX/macro context and funding crowding can be compared with later BTC return and absolute-move distributions | `/v1/market/quotes`, `/v1/market/perpetual-funding`, JSONL archive | Snapshot response study; macro timestamps, causality, costs and execution remain explicit gaps |
| `crypto/macro/crypto_etf_flow_response_recorder.py` / `crypto_etf_flow_response_replay.py` | Large daily BTC ETF inflows/outflows may have a different next-window BTC response than ordinary flow days | MarketBridge `farside_etf` external signals, `/v1/market/quotes`, `/v1/history/candles`, or Farside-style CSV | External-flow response study; historical ingestion, NAV timing, revisions, causality and execution remain explicit gaps |
| `crypto/sentiment/crypto_sentiment_extremes_monitor.py` / recorder / replay | Extreme Fear/Greed states may have a different fixed-horizon BTC response distribution than ordinary windows | `/v1/external/signals?sources=fear_greed`, `/v1/market/quotes` and JSONL archive | Descriptive forward-response replay; provider composite, sample alignment and paper costs remain explicit |
| `crypto/sentiment/crypto_news_attention_monitor.py` / recorder / replay | A burst of high-score CryptoPanic items may precede larger absolute BTC movement than ordinary windows | `/v1/external/signals?sources=cryptopanic&categories=news`, `/v1/market/quotes` and JSONL archive | Non-directional attention replay; feed coverage, vote semantics and timing remain explicit |
| `crypto/sentiment/crypto_social_signal_response_recorder.py` / `crypto_social_signal_response_replay.py` | A change in a keyed LunarCrush/Santiment metric may be followed by a different absolute BTC response than ordinary snapshots | `/v1/external/signals`, `/v1/market/quotes`, JSONL archive | Provider-specific social-score response study; API key, metric scale, coverage and execution remain explicit gaps |
| `funding_convergence_monitor.py` | Compare explicit hourly funding rates across venues and flag a gross differential for investigation | `/v1/market/perpetual-funding` | Withholds annualization when provider interval is unknown; no hedge execution |
| `funding_convergence_replay.py` | Align historical funding observations and measure gross and after-cost differential persistence across venues | `/v1/market/perpetual-funding`, `/v1/history/candles` | Explicit paper cost hurdle is a sensitivity input; no fill, borrow or hedge simulation |
| `crypto_funding_oi_replay.py` | Extreme funding plus rising OI may identify crowded longs/shorts whose next price window moves against the crowd | `/v1/history/candles`, `/v1/history/open-interest` | Venue and schedule gaps remain explicit; forward return is not a hedge PnL |
| `crypto/carry/crypto_funding_regime_replay.py` | Persistent same-direction extreme funding may precede a move against the crowded side | `/v1/history/candles` for funding-rate and perp candles | Funding-only persistence replay; no OI, funding income, hedge or execution model |
| `crypto/carry/crypto_funding_cross_section_replay.py` | At wide funding dispersion, low-funding assets may have different next-window returns from high-funding assets | `/v1/history/candles` for funding-rate and perp candles across a caller-selected universe | Cross-sectional diagnostic with freshness, exact price intersections and optional paper cost; no allocation or hedge execution |
| `crypto/carry/crypto_cross_venue_price_gap_replay.py` | An extreme same-asset log-price gap across two venues may contract toward its frozen trailing mean | `/v1/history/candles` for the same symbol on two venues, exact timestamp intersection | Price-fragmentation diagnostic; synchronized fills, inventory, transfers, fees and execution remain explicit gaps |
| `crypto/carry/crypto_cross_venue_orderbook_monitor.py` / recorder / replay | A synchronized target-notional ask/bid VWAP gap may persist after a paper round-trip cost hurdle | `/v1/market/order-books` and JSONL archive | Snapshot depth diagnostic; timestamp skew, inventory, settlement, transfer and execution remain explicit gaps |
| `crypto/carry/crypto_cross_venue_orderbook_response_recorder.py` / `crypto_cross_venue_orderbook_response_replay.py` | A qualifying synchronized book edge may have a different later BTC response than an unqualified snapshot | `/v1/market/order-books`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; no simultaneous fills, arbitrage PnL, inventory or execution claim |
| `crypto/carry/crypto_triangular_arbitrage_monitor.py` / recorder / replay | A synchronized single-venue three-leg top-of-book conversion edge may persist after paper per-leg costs | `/v1/market/quotes?product_type=spot` for `BTCUSDT`, `ETHBTC`, `ETHUSDT`, plus JSONL archive | Quote-consistency persistence diagnostic; depth, atomicity, latency, inventory and execution remain explicit gaps |
| `crypto/carry/crypto_triangular_arbitrage_response_recorder.py` / `crypto_triangular_arbitrage_response_replay.py` | A qualifying three-leg quote edge may have a different later BTC response than an unqualified snapshot | `/v1/market/quotes?product_type=spot`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; no atomic fills, triangular PnL, depth, inventory or execution claim |
| `crypto/carry/crypto_positioning_regime_replay.py` | Price trend, OI change and funding sign may separate forward-return distributions | `/v1/history/candles`, `/v1/history/open-interest` | Point-in-time regime matrix; OI is aggregate and no long/short ownership is inferred |
| `crypto/carry/crypto_oi_impulse_response_recorder.py` / `crypto_oi_impulse_response_replay.py` | An unusually large OI expansion may be followed by larger absolute price movement, as liquidation-risk context rather than direction | `/v1/market/open-interest`, `/v1/market/quotes`, JSONL archive | Expansion/contraction response study; OI ownership, elapsed-time alignment and execution remain explicit gaps |
| `crypto_microstructure_monitor.py` | Top-of-book bid/ask depth imbalance can identify short-term pressure, while extreme funding is a crowding warning | `/v1/market/order-books`, `/v1/market/perpetual-funding` | Snapshot observer; missing books and funding conflicts stay explicit |
| `crypto_flow_book_confirmation.py` | Same-direction taker-flow delta/CVD confirms an L2 pressure candidate; opposite flow rejects it | `/v1/market/order-books`, `/v1/market/order-flow`, `/v1/market/perpetual-funding` | Point-in-time confirmation observer; no execution, fill or cost model |
| `crypto_spot_perp_depth_gap_monitor.py` | Same-venue perp depth may exceed spot depth for a target notional, creating an execution-risk asymmetry | `/v1/market/order-books` for spot/perp, `/v1/market/basis` | Snapshot observation; books are not synchronized fills and no hedge route is inferred |
| `crypto_spot_perp_depth_gap_recorder.py` / `crypto_spot_perp_depth_gap_replay.py` | Test whether a target-size perp/spot depth advantage persists across snapshots | JSONL from the same order-book/basis endpoints | Persistence diagnostic; no route, hedge, fill or capacity model |
| `crypto/microstructure/crypto_spot_perp_depth_gap_response_recorder.py` / `crypto_spot_perp_depth_gap_response_replay.py` | Compare later BTC movement after perp-depth advantage, spot-depth advantage and no-gap states | `/v1/market/order-books`, `/v1/market/basis`, `/v1/market/quotes`, JSONL archive | Fixed-record response study; no hedge route, arbitrage PnL or execution claim |
| `crypto_liquidity_stress_monitor.py` | Target-size book impact, quoted spread and short-horizon EWMA volatility can identify a stressed unwind regime | `/v1/market/order-books`, `/v1/history/candles` | Two-of-three risk context; no direction, routing or sizing decision |
| `crypto/microstructure/crypto_liquidity_stress_recorder.py` / `crypto_liquidity_stress_replay.py` | Test whether a two-of-three liquidity-stress state persists across snapshots | JSONL from order-book and candle observations | Persistence diagnostic; missing inputs stay outside coverage, no routing or execution model |
| `crypto/microstructure/crypto_liquidity_stress_response_recorder.py` / `crypto_liquidity_stress_response_replay.py` | Compare later BTC movement after liquidity-stress, watch and normal snapshots | `/v1/market/order-books`, `/v1/history/candles`, `/v1/market/quotes`, JSONL archive | Fixed-record risk-context response study; no directional, routing or execution claim |
| `python_strategy_runner.py` | Shared Python implementation for categorized crypto observers, including liquidity stress | normalized MarketBridge endpoints | Primary strategy implementation; read-only JSON output; requests only the selected strategy's inputs |
| `funding_extremes.py` | Extreme funding is a candidate discovery filter, not a directional signal | on-demand perpetual funding | Research utility |
| `funding_curve_demo.py` | Funding-rate persistence and extreme runs should be examined across time | funding-rate history | Research visualization |

## Run the cases

Start a read-only live configuration first, then run one observer:

```bash
export MARKETBRIDGE_CONFIG=./config.squeeze-radar.local.yaml
cargo run
```

For the BTC microstructure monitor specifically, the bounded
`config.research-live.yaml` enables both spot and BTC perpetual public feeds:

```bash
MARKETBRIDGE_CONFIG=./config.research-live.yaml cargo run
```

```bash
python3 examples/crypto/microstructure/short_squeeze_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/microstructure/exhaustion_short_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/carry/basis_carry_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/crypto/carry/crypto_basis_recorder.py \
  --symbol BTCUSDT --exchanges binance,okx --iterations 120 --interval-secs 30 \
  --output work/crypto-basis.jsonl
python3 examples/crypto/carry/crypto_basis_replay.py \
  --input work/crypto-basis.jsonl --symbol BTCUSDT --lookback 20 --horizon 3
python3 examples/crypto/microstructure/liquidation_reversal_monitor.py \
  --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/liquidation_reversal_replay.py \
  --exchange okx --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000 \
  --oi-exchange bybit --trades-exchange okx
python3 examples/liquidation_reversal_replay.py \
  --exchange coinex --price-exchange okx --symbol BTCUSDT --limit 100 \
  --horizon-bars 3 --min-notional 100000 \
  --oi-exchange bybit --trades-exchange okx
python3 examples/polymarket_complement_monitor.py --min-edge-bps 10
python3 examples/polymarket_price_shock_replay.py \
  --market-query "temperature" --outcome Yes --interval 1m \
  --shock-bps 100 --horizon-points 3 --min-observations 5
python3 examples/polymarket_timing_replay.py \
  --market-query "Bitcoin above" --market-limit 100 \
  --min-bucket-observations 5 --position-size-usd 10 --fee-bps 30
python3 examples/prediction_trade_flow.py --market 0x... --limit 1000
python3 examples/polymarket_settlement_replay.py \
  --market 0x... --max-entry-price 0.80 --fee-bps 30
python3 examples/polymarket_trade_recorder.py \
  --market 0x... --page-size 1000 --pages 3 \
  --output work/polymarket-trades.jsonl
python3 examples/polymarket_settlement_replay.py \
  --market 0x... --trades-jsonl work/polymarket-trades.jsonl
python3 examples/polymarket_calibration_report.py \
  --trades-jsonl work/polymarket-trades.jsonl --resolved-outcome No
python3 examples/weather_event_observer.py \
  --latitude 52.52 --longitude 13.41 --date 2026-09-14 \
  --min-temp 15 --max-temp 25
python3 examples/weather_pressure_differential.py \
  --market-query "Berlin temperature" \
  --latitude 52.52 --longitude 13.41 --date 2026-09-14 \
  --min-temp 15 --max-temp 25 --max-yes-ask 0.25
python3 examples/weather_market_calibration.py \
  --manifest examples/weather-market-manifest.example.jsonl
python3 examples/crypto/microstructure/crypto_session_filter.py \
  --exchange binance --market perp --symbol BTCUSDT \
  --interval 1m --limit 60 --timezone America/New_York
python3 examples/crypto/microstructure/crypto_anchored_vwap_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 5m \
  --anchor-lookback 96 --anchor-mode both --horizon-bars 12 \
  --volume-multiplier 1.0 --paper-cost-bps 10 --min-observations 5
python3 examples/crypto_volatility_breakout_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 5m \
  --days 3 --range-bars 12 --compression-window 12 \
  --baseline-window 48 --flow-exchange binance --flow-pages 12
python3 examples/crypto/microstructure/crypto_cvd_divergence_replay.py \
  --exchange binance --trades-exchange binance --symbol BTCUSDT \
  --interval 5m --lookback-bars 12 --horizon-bars 3 \
  --min-price-move-pct 0.5 --min-flow-ratio 0.2 \
  --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_monitor.py \
  --symbol BTC --long-short-high 1.2 --long-short-low 0.8
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_recorder.py \
  --symbol BTC --iterations 20 --interval-secs 30 \
  --output work/crypto-derivatives-sentiment.jsonl
python3 examples/crypto/microstructure/crypto_derivatives_sentiment_replay.py \
  --input work/crypto-derivatives-sentiment.jsonl --min-run 3
python3 examples/crypto/microstructure/crypto_bollinger_squeeze_replay.py \
  --exchange binance --symbol BTCUSDT --interval 5m --days 7 \
  --period 20 --deviations 2 --bandwidth-lookback 96 \
  --max-bandwidth-quantile 0.20 --horizon-bars 12 \
  --paper-cost-bps 10 --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_vwap_deviation_reversion_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1h --days 30 \
  --deviation-bps 50 --sigma 2 --horizon-bars 12 \
  --paper-cost-bps 10 --min-edge-bps 0 --min-observations 5
python3 examples/crypto/microstructure/crypto_short_squeeze_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 30 \
  --output work/crypto-short-squeeze-response.jsonl
python3 examples/crypto/microstructure/crypto_short_squeeze_response_replay.py \
  --input work/crypto-short-squeeze-response.jsonl --horizon-records 7 \
  --min-score 3 --min-observations 5 --paper-cost-bps 10
python3 examples/crypto/microstructure/crypto_derivatives_crowding_response_recorder.py \
  --symbol BTC --price-symbol BTCUSDT --exchange binance --product-type perp \
  --iterations 30 --interval-secs 30 \
  --output work/crypto-derivatives-crowding-response.jsonl
python3 examples/crypto/microstructure/crypto_derivatives_crowding_response_replay.py \
  --input work/crypto-derivatives-crowding-response.jsonl \
  --horizon-records 7 --min-observations 5 --paper-cost-bps 10
python3 examples/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --min-skew-iv 3 --min-term-slope-iv 3
python3 examples/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-skew-iv 3 --min-run 3
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
python3 examples/crypto/defi/crypto_defi_pool_flow_monitor.py \
  --sources uniswap_v3,meteora --min-liquidity-usd 100000 \
  --min-turnover-h1 0.25
python3 examples/crypto/defi/crypto_defi_pool_flow_recorder.py \
  --sources uniswap_v3,meteora --iterations 30 --interval-secs 30 \
  --output work/crypto-defi-pool-flow.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_replay.py \
  --input work/crypto-defi-pool-flow.jsonl --min-run 3
python3 examples/crypto/defi/crypto_defi_pool_flow_response_recorder.py \
  --sources uniswap_v3,meteora --price-exchange binance --price-symbol BTCUSDT \
  --iterations 120 --interval-secs 30 \
  --output work/crypto-defi-pool-flow-response.jsonl
python3 examples/crypto/defi/crypto_defi_pool_flow_response_replay.py \
  --input work/crypto-defi-pool-flow-response.jsonl \
  --horizon-records 3 --min-observations 10
python3 examples/crypto/onchain/crypto_onchain_transfer_burst_replay.py \
  --source whale_alert --asset USDT --min-transfer-usd 100000 \
  --price-exchange binance --symbol BTCUSDT --interval 5m \
  --threshold-usd 1000000 --window-hours 24 --horizon-bars 12
python3 examples/crypto/onchain/crypto_onchain_transfer_response_recorder.py \
  --source whale_alert --asset USDT --min-transfer-usd 100000 \
  --price-exchange binance --price-symbol BTCUSDT \
  --iterations 120 --interval-secs 60 \
  --output work/crypto-onchain-transfer-response.jsonl
python3 examples/crypto/onchain/crypto_onchain_transfer_response_replay.py \
  --input work/crypto-onchain-transfer-response.jsonl \
  --window-hours 24 --horizon-records 12 \
  --threshold-usd 1000000 --min-observations 3
python3 examples/crypto_options_vrp_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --price-exchange binance --symbol BTCUSDT --interval 1h --rv-bars 168 \
  --vrp-threshold 5
python3 examples/crypto_options_gamma_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --min-near-share 0.50 --min-concentration 0.10 \
  --max-book-fetches 24
python3 examples/crypto_options_gamma_recorder.py \
  --currency BTC --venue deribit --iterations 20 --interval-secs 30 \
  --output work/crypto-options-gamma.jsonl
python3 examples/crypto_options_gamma_replay.py \
  --input work/crypto-options-gamma.jsonl --min-near-share 0.50 \
  --min-concentration 0.10 --min-run 3
python3 examples/crypto/options/crypto_options_gamma_response_recorder.py \
  --currency BTC --venue deribit --price-symbol BTCUSDT --exchange binance \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-gamma-response.jsonl
python3 examples/crypto/options/crypto_options_gamma_response_replay.py \
  --input work/crypto-options-gamma-response.jsonl --horizon-records 3 \
  --min-near-share 0.50 --min-concentration 0.10 --min-observations 5
python3 examples/crypto_universe_opportunity_scan.py \
  --exchange binance --market perp --interval 5m \
  --min-quote-volume 1000000 --min-realized-vol 0.2 \
  --min-abs-funding-hourly-pct 0.01 --min-score 2
python3 examples/crypto_cross_asset_momentum_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance \
  --interval 1h --lookback-bars 8 --horizon-bars 8 \
  --top-k 1 --min-observations 5
python3 examples/funding_convergence_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit \
  --iterations 3 --interval-secs 30
python3 examples/funding_convergence_replay.py \
  --symbol BTCUSDT --exchanges binance,bybit --days 7 --limit 200
python3 examples/crypto_funding_oi_replay.py \
  --symbol BTCUSDT --funding-exchange binance \
  --oi-exchange binance --price-exchange binance \
  --days 7 --min-funding-pct 0.01 --min-oi-change-pct 0.10
python3 examples/crypto/carry/crypto_funding_regime_replay.py \
  --symbol BTCUSDT --funding-exchange binance --price-exchange binance \
  --days 14 --min-funding-pct 0.01 --min-run 3 --horizon-bars 3
python3 examples/crypto/carry/crypto_funding_cross_section_replay.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --funding-exchange binance \
  --price-exchange binance --interval 1h --days 14 --top-k 1 \
  --min-dispersion-bps 1 --paper-cost-bps 10 --min-edge-bps 0
python3 examples/crypto/carry/crypto_oi_impulse_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --min-oi-change-pct 0.25 --output work/crypto-oi-impulse-response.jsonl
python3 examples/crypto/carry/crypto_oi_impulse_response_replay.py \
  --input work/crypto-oi-impulse-response.jsonl --horizon-records 7 \
  --min-oi-change-pct 0.25 --min-observations 5
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
python3 examples/crypto_microstructure_monitor.py \
  --symbol BTCUSDT --exchange binance --top-levels 5 \
  --imbalance-threshold 0.30 --funding-extreme-pct 0.01
python3 examples/crypto_flow_book_confirmation.py \
  --symbol BTCUSDT --exchange binance --window-ms 60000 \
  --imbalance-threshold 0.30 --flow-threshold 0.20
python3 examples/python_strategy_runner.py \
  --strategy squeeze --symbol BTCUSDT --exchange binance --iterations 3
python3 examples/python_strategy_runner.py \
  --strategy options_skew --currency BTC --options-venue deribit \
  --expiry-days 30 --iterations 2 --interval-secs 30
python3 examples/python_strategy_runner.py \
  --strategy options_vrp --currency BTC --options-venue deribit \
  --symbol BTCUSDT --exchange binance --rv-interval 1h --rv-bars 168
python3 examples/python_strategy_runner.py \
  --strategy volatility_breakout --symbol BTCUSDT --exchange binance \
  --breakout-interval 5m --breakout-limit 100 \
  --range-bars 12 --compression-window 12 --baseline-window 48
python3 examples/python_strategy_runner.py \
  --strategy funding_convergence --symbol BTCUSDT \
  --funding-exchanges binance,okx,bybit \
  --min-spread-bps-per-hour 0.5
python3 examples/python_strategy_runner.py \
  --strategy cross_asset_momentum --exchange binance \
  --cross-asset-symbols BTCUSDT,ETHUSDT,SOLUSDT \
  --cross-asset-interval 1h --cross-asset-lookback 8 \
  --cross-asset-horizon 8 --cross-asset-top-k 1
python3 examples/crypto/universe/crypto_altcoin_breadth_replay.py \
  --btc-symbol BTCUSDT --alt-symbols ETHUSDT,SOLUSDT,BNBUSDT,XRPUSDT,ADAUSDT \
  --exchange binance --market perp --interval 1d --lookback-bars 90 \
  --horizon-bars 7 --low-threshold 0.25 --high-threshold 0.75 \
  --min-alt-assets 3 --min-observations 5 --paper-cost-bps 20
python3 examples/crypto/universe/crypto_volatility_adjusted_momentum_sweep.py \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT --exchange binance --interval 1h \
  --lookback-bars 4,8,12 --volatility-bars 4,8,12 --horizon-bars 4,8 \
  --roundtrip-cost-bps 20
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
python3 examples/crypto/universe/crypto_market_regime_monitor.py \
  --symbols BTCUSDT,ETHUSDT --intervals 1h,4h,1d
python3 examples/crypto/universe/crypto_market_regime_recorder.py \
  --symbols BTCUSDT,ETHUSDT --exchange binance --market perp \
  --intervals 1h,4h,1d --price-symbol BTCUSDT --iterations 30 \
  --interval-secs 30 --output work/crypto-market-regime.jsonl
python3 examples/crypto/universe/crypto_market_regime_replay.py \
  --input work/crypto-market-regime.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 10
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
python3 examples/crypto/sentiment/crypto_sentiment_extremes_monitor.py \
  --symbol BTCUSDT --exchange binance --fear-max 20 --greed-min 80
python3 examples/crypto/sentiment/crypto_sentiment_extremes_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 86400 \
  --output work/crypto-sentiment-extremes.jsonl
python3 examples/crypto/sentiment/crypto_sentiment_extremes_replay.py \
  --input work/crypto-sentiment-extremes.jsonl --horizon-records 7 \
  --min-observations 5 --paper-cost-bps 20
python3 examples/crypto/sentiment/crypto_news_attention_monitor.py \
  --symbol BTCUSDT --exchange binance --min-score 3 --min-items 3
python3 examples/crypto/sentiment/crypto_news_attention_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 120 --interval-secs 300 \
  --output work/crypto-news-attention.jsonl
python3 examples/crypto/sentiment/crypto_news_attention_replay.py \
  --input work/crypto-news-attention.jsonl --horizon-records 6 \
  --min-observations 5
python3 examples/crypto/sentiment/crypto_social_signal_response_recorder.py \
  --source lunarcrush --metric lunarcrush_social_score \
  --signal-symbol BTC --price-symbol BTCUSDT --exchange binance \
  --iterations 30 --interval-secs 3600 --min-change 1 \
  --output work/crypto-social-response.jsonl
python3 examples/crypto/sentiment/crypto_social_signal_response_replay.py \
  --input work/crypto-social-response.jsonl --horizon-records 6 \
  --min-change 1 --min-observations 5
python3 examples/python_strategy_runner.py \
  --strategy options_gamma --currency BTC --options-venue deribit \
  --expiry-days 30 --gamma-min-near-share 0.50 \
  --gamma-min-concentration 0.10
python3 examples/funding_extremes.py --exchange binance --min-pct -2 --max-pct -0.1
```

The first OI poll has no change baseline. A missing book, funding row, or
liquidation feed is an evidence gap, not a zero and not a trade instruction.
The same rule applies to the session filter: a cold-start or disabled kline
store returns structured `insufficient_klines` evidence instead of a signal.

## Strategy provenance and next cases

The initial cases are deliberately tied to public strategy discussions, then
rewritten as falsifiable hypotheses:

- [Basis trade discussion by CryptoCred](https://x.com/CryptoCred/status/1777720296297975952)
- [CME short / spot buy basis example](https://x.com/0xscarlettw/status/1944584946670276938)
- [Liquidation and OI reversal thesis](https://x.com/TheCryptoData/status/1948466627365769584)
- [OI, flow confirmation and short-covering discussion](https://x.com/xwinfinance/status/2023155692916646257)
- [Resolved-market replay and calibration-arbitrage discussion](https://x.com/AlterEgo_eth/status/2040417268656644512)
- [Weather pressure-differential narrative (unverified public claim)](https://x.com/kiruwaaaaaa/status/2032525403320160313)
- [Bayesian event-arb / faster evidence update narrative (unverified public claim)](https://x.com/0xRicker/status/2035334040216113631)
- [15-minute Polymarket timing, early price discovery vs late conviction narrative (unverified public claim)](https://x.com/telonex/status/2022251717270573513)
- [15-minute session, VWAP/EMA/MACD/volume narrative (unverified public claim)](https://x.com/Gustafssonkotte/status/2030566353178882122)
- [Anchored VWAP technical-analysis discussion (unverified public claim)](https://x.com/Jake__Wujastyk/status/1873917626638098894)
- [Cross-venue funding differential narrative (unverified public claim)](https://x.com/leondoteth/status/2012127303850213817)
- [Funding/OI/liquidation context snapshot (unverified public claim)](https://x.com/ImCryptOpus/status/1949195275903410571)
- [Macro liquidity, ETF-flow and crypto-regime context (unverified public claim)](https://x.com/wintermute_t/status/1985631560021000352)
- [Crowded positioning and liquidation-to-reversal context (unverified public claim)](https://x.com/TheCryptoData/status/1948466627365769584)
- [L2 imbalance plus funding-extreme perp logic (unverified public claim)](https://x.com/instaclaws/status/2038363051213181035)
- [Realized-volatility compression context (unverified public claim)](https://x.com/glassnode/status/1955218957490594099)
- [Breakout confirmation / hold-above-level context (unverified public claim)](https://x.com/rektcapital/status/1893996786173259958)
- [BTC/ETH ATM IV and 25D skew options brief (unverified public claim)](https://x.com/Gate_Launch/status/2063810805552845140)
- [Deribit bull-call-spread / options-flow observation (unverified public claim)](https://x.com/laevitas1/status/1985373005644476891)
- [IV minus realized-volatility dashboard / VRP context (unverified public claim)](https://x.com/isellpremium/status/2072350364385349678)
- [Volatility-adjusted multi-asset BTC/ETH/SOL strategy context (unverified public claim)](https://x.com/RoboNetHQ/status/2024893544520143012)
- [xWIN altcoin-index / breadth context (unverified public claim)](https://x.com/xwinfinance/status/1951412106345193606)
- [Compression-to-expansion / low-volume-node context (unverified public claim)](https://x.com/Stoiiic/status/1796078958674628714)
- [Binance public order-book API documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data)
- [Cross-exchange arbitrage friction and settlement-latency study](https://academic.oup.com/rof/article/28/4/1345?guestAccessKey=50540e27-1995-48e8-bb51-6b93b219d2ad)

Next additions are ordered by evidence value: deeper venue-specific public
liquidation coverage, larger verified weather manifests, and point-in-time
market identity joins. A new case is accepted only after its inputs, costs,
invalidation rule and replay window are recorded.
