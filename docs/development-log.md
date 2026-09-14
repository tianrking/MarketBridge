# Development log

## 2026-09-14 — Options max-pain proxy and expiry response study

Added the Python-first `crypto_options_max_pain_monitor.py` family with
persistence and BTC-response recorder/replay pairs. It calculates a
provider-unit intrinsic-pain proxy for each expiry from strike and open
interest, labels near-expiry proximity versus dislocation, and compares later
fixed-record BTC responses. The result is explicitly not settlement PnL,
price-pinning proof, dealer intent, or execution.

Provenance: the public [max-pain discussion on X](https://x.com/martypartymusic/status/1971319091915829353)
is an unverified research lead. The calculation and expiry caveats follow
[Deribit's max-pain explanation](https://insights.deribit.com/education/maximum-pain-for-option-buyers-going-into-expiration/),
[Deribit's Python max-pain example](https://insights.deribit.com/dev-hub/deribit-max-pain-python-code/),
and the [Deribit Metrics documentation](https://insights.deribit.com/education/introduction-to-deribit-metrics-page/).
Provider units, TWAP settlement, expiry roll, missing OI, option PnL, hedge,
margin, cost and execution remain explicit gaps.

## 2026-09-14 — Options put/call open-interest response study

Added the Python-first `crypto_options_put_call_oi_monitor.py` family with a
JSONL persistence recorder/replay and a BTC-response recorder/replay. It sums
provider-reported call and put open interest within an explicit expiry window,
labels defensive-put, call-dominant and balanced states, and compares later
fixed-record BTC responses without treating the ratio as dealer positioning.

Provenance: the public [options risk-management lead on X](https://x.com/Gate_Launch/status/2063810805552845140)
is an unverified research lead. Put/call OI and expiry grouping follow
[Deribit Metrics documentation](https://insights.deribit.com/education/introduction-to-deribit-metrics-page/)
and the [Deribit ticker field semantics](https://docs.deribit.com/api-reference/upcoming/market-data/public-ticker).
The case preserves provider units, expiry roll, missing OI, option PnL, hedge,
margin, cost and execution as explicit gaps.

## 2026-09-14 — Funding-sensitive DeFi yield risk context

Added `crypto_defi_funding_yield_risk_monitor.py`, its JSONL recorder/replay,
and deterministic tests. The case joins the existing DefiLlama yield context
with on-demand perpetual funding rows, annualizing each rate only when the
provider reports a funding interval. Reward-heavy pools are labeled
`funding_sensitive_yield_risk` when the selected funding basket is negative;
the replay requires a persistent run before reporting a candidate.

Provenance: the public [CIAN sUSDe funding/exit-risk discussion](https://x.com/CIAN_protocol/status/2032403312621007326)
is an unverified research lead. The mechanism is cross-checked against
[Ethena's USDe documentation](https://docs.ethena.fi/how-usde-works),
[Funding Risk](https://docs.ethena.fi/solution-overview/risks/funding-risk),
and the [sUSDe rewards mechanism](https://docs.ethena.fi/solution-overview/protocol-revenue-explanation/susde-rewards-mechanism).
The selected funding symbols are a transparent proxy, not a protocol hedge
book; no accounting, yield guarantee, deposit, redemption, wallet or execution
path is added.

## 2026-09-14 — DeFi yield context and bilingual boundary cleanup

Added the read-only `/v1/external/defi-yields` endpoint backed by DefiLlama's
public `/pools` snapshot. It filters by chain, project, symbol, stablecoin,
TVL and APY, and preserves total/base/reward APY, TVL, provider changes and
pool-risk flags. Added the Python-first
`crypto_defi_yield_context_monitor.py` case and deterministic tests; it
separates base-yield-dominant, reward-dependent, non-positive and missing-data
states. APY/TVL remain provider observations, not guaranteed return, safety or
redemption liquidity. No deposit, withdrawal, wallet signing or execution path
was added.

Provenance: [DefiLlama yields](https://defillama.com/yields) and the
[DefiLlama API documentation](https://defillama.com/docs/api). The root
English and Chinese READMEs now present the same boundary table, short entry
commands and API links; the long research history remains folded below the
boundary section.

## 2026-09-14 — Deribit volatility-index history and response replay

Added the read-only `/v1/history/volatility-index` endpoint backed by
Deribit's public `public/get_volatility_index_data` JSON-RPC method. It
normalizes bounded OHLC candles, supported currencies/resolutions, provider
continuation and explicit coverage metadata. Added
`crypto_deribit_volatility_index_response_replay.py`, which classifies high,
low and ordinary index states and compares later fixed-window BTC absolute
responses against the ordinary bucket. It does not claim a complete option
surface, a forecast, option PnL, hedging or execution.

Provenance: the public [options-volatility regime lead on X](https://x.com/Gate_Launch/status/2063810805552845140)
is an unverified research lead. Field and resolution semantics are cross-checked
against [Deribit's official volatility-index history documentation](https://docs.deribit.com/api-reference/market-data/public-get_volatility_index_data).

## 2026-09-14 — Deribit volatility-index minus RV response study

Added `crypto_deribit_volatility_index_vrp_response_replay.py` as a separate
research case over the new volatility-index endpoint. It computes an explicit
close-to-close annualized RV window from MarketBridge perp candles, classifies
index-minus-RV premium, discount and aligned states, and compares later fixed
BTC responses against the aligned bucket. Different constructions, horizons,
missing candles, option PnL, hedging and execution remain explicit gaps.

Provenance: the public [volatility-risk-premium research lead on X](https://x.com/ConcretumR/status/1952298941745172695)
is treated as a falsifiable lead only; the Deribit OHLC semantics remain
grounded in the [official volatility-index API documentation](https://docs.deribit.com/api-reference/market-data/public-get_volatility_index_data).

## 2026-09-14 — Cross-asset Deribit volatility spread response study

Added `crypto_deribit_cross_asset_volatility_response_replay.py`. It aligns
BTC and ETH public Deribit volatility-index closes, classifies the ETH-minus-BTC
spread, and compares later ETH-minus-BTC perp returns and absolute relative
responses against an aligned control state. It keeps provider timestamp
intersection, missing price windows, option PnL, hedge ratio, fees and
execution outside the claim.

Provenance: the public [BTC/ETH options comparison on X](https://x.com/Gate_Launch/status/2063810805552845140)
is an unverified lead; index fields remain grounded in [Deribit's official
volatility-index API documentation](https://docs.deribit.com/api-reference/market-data/public-get_volatility_index_data).

## 2026-09-14 — Binance global account-ratio scope

Extended `/v1/history/account-ratio` with `scope=global|top_trader` for
Binance. The normalized rows retain distinct `source` and `semantics` values,
so all-account long/short shares cannot be merged with top-trader account
shares. Updated `crypto_account_ratio_oi_response_replay.py` to select the
Binance scope while preserving its OI-aligned, fixed-horizon response study.
Provenance: [Binance Long/Short Ratio](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Long-Short-Ratio)
and [Binance Top Trader Long/Short Account Ratio](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Top-Long-Short-Account-Ratio).
No notional-position inference, order, wallet, signing or execution path is included.

## 2026-09-14 — CoinGecko global market context monitor

Added the read-only `/v1/external/global-market` endpoint, normalizing
CoinGecko's total market cap/volume, BTC/ETH dominance, 24-hour changes, active
cryptocurrency count, market count and provider timestamp. Added the Python
`crypto_global_market_regime_monitor.py` case, which classifies stress,
BTC-dominant, broad-risk-on and mixed context without turning one snapshot into
a strategy signal. Provenance: [CoinGecko Crypto Global Market Data](https://docs.coingecko.com/reference/crypto-global),
[CoinGecko market research guidance](https://docs.coingecko.com/docs/market-research),
and the unverified [public X BTC-dominance lead](https://x.com/1881Erdem/status/2041949708864643566).
No order, wallet, signing or execution path is included.

The same round added `crypto_global_market_regime_recorder.py` and
`crypto_global_market_regime_replay.py`. The recorder stores the provider
snapshot beside a synchronized BTC quote; the replay compares fixed-record
forward return, absolute move and downside distributions by global regime.
Missing historical dominance coverage, exact elapsed time and execution costs
remain explicit evidence gaps.

## 2026-09-14 — Binance funding provider-band context

Extended on-demand perpetual funding rows with Binance's published adjusted
funding-rate cap, floor and interval when `fundingInfo` exposes them. Added the
Python `crypto_funding_band_monitor.py` case, which makes the falsifiable
hypothesis explicit: cap/floor proximity may define a later response state, but
it is not a direction, funding-income or execution claim. Provenance: [Binance
Funding Rate Info API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Get-Funding-Info)
and the unverified [public X funding-extremes lead](https://x.com/instaclaws/status/2038363051213181035).
No order, wallet, signing or execution path is included.

## 2026-09-14 — Binance ADL-risk context monitor

Added the read-only `/v1/market/adl-risk` endpoint for Binance's public
symbol-level ADL risk rating. The normalized snapshot preserves high/medium/low
provider state, update time and coverage; it is explicitly kept separate from
directional strategy signals. Added the Python
`crypto_adl_risk_monitor.py` observer and tests. Provenance: [Binance ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk),
which describes the rating as a provider risk context updated about every 30
minutes. No private account data, order, wallet, signing or execution path is
included.

## 2026-09-14 — Binance historical basis convergence input

Added the read-only `/v1/history/basis` endpoint for Binance's public futures
basis history. Rows normalize provider basis, basis rate, annualized basis rate,
index/futures prices and timestamp with bounded coverage metadata. Added the
Python `crypto_historical_basis_replay.py` case, which compares contraction
after extreme basis-rate observations with ordinary observations. Provenance:
[Binance Basis API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Basis)
and the public [CryptoCred basis discussion on X](https://x.com/CryptoCred/status/1777720296297975952).
No simultaneous fill, hedge, carry PnL, order, wallet, signing or execution path
is included.

## 2026-09-14 — Binance top-trader account ratio coverage

Extended `/v1/history/account-ratio` to Binance's public top-trader account
ratio endpoint. Rows retain `source` and `semantics` so Binance top-account
shares are not conflated with Bybit holder-count shares; both remain bounded
research context for the account-ratio/OI response replay. Provenance:
[Binance Top Trader Long/Short Account Ratio](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Top-Trader-Long-Short-Ratio).
No notional-position inference, order, wallet, signing or execution path is
included.

## 2026-09-14 — Bybit historical option-volatility response input

Added the read-only `/v1/history/historical-volatility` endpoint for Bybit's
public option historical-volatility values. It validates supported periods,
base/quote parameters and paired time bounds, then normalizes provider values
with timestamps and coverage metadata. Added the Python
`crypto_historical_volatility_response_replay.py` case, which compares high,
low and ordinary provider-volatility states with later bounded BTC movement.
Provenance: [Bybit Get Historical Volatility](https://bybit-exchange.github.io/docs/v5/market/iv)
and the public [Glassnode realized-volatility observation on X](https://x.com/glassnode/status/1955218957490594099).
No implied-volatility forecast, option PnL, order, wallet, signing or execution
path is included.

## 2026-09-14 — premium-index/funding response replay

Added the Python carry-family `crypto_premium_funding_response_replay.py` case.
It aligns Binance `premiumIndex` candles, funding-rate observations and perp
candles, labels opposite-sign divergence versus aligned/ordinary states, and
compares fixed-horizon signed and absolute responses. The example uses the
existing read-only history API and keeps provider page limits, as-of age and
missing alignment explicit. Provenance: [Binance Premium Index Kline API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Premium-Index-Kline-Data)
and the public [cross-venue funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817).
No funding-income, hedge, order or execution path is included.

## 2026-09-14 — Bybit account-ratio/OI response research input

Added the read-only `/v1/history/account-ratio` endpoint for Bybit's public
holder-count long/short ratio history. The normalized rows expose buy/sell
ratios, signed imbalance and provider cursor/coverage metadata. Added the
Python `crypto_account_ratio_oi_response_replay.py` case, which aligns the
ratio with aggregate OI and candles to compare crowding and unwinding states.
Provenance: [Bybit Get Long Short Ratio](https://bybit-exchange.github.io/docs/v5/market/long-short-ratio).
The ratio is not notional positioning or trader intent; no order, wallet,
signing or execution path is included.

## 2026-09-14 — Binance taker-volume/OI response research input

Added the read-only `/v1/history/taker-volume` endpoint for Binance's public
aggregate taker buy/sell volume history. Rows now expose normalized buy/sell
volume, value, total and signed imbalance with bounded coverage metadata. Added
the Python `crypto_taker_oi_response_replay.py` case, which separates rising-OI
pressure from falling-OI absorption/closing states and compares fixed-horizon
price responses. Provenance: [Binance Taker Buy/Sell Volume API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/Taker-BuySell-Volume).
No trader identity, causal inference, order, wallet, signing or execution path
is included.

## 2026-09-14 — Meteora DLMM native pool context

Replaced the Meteora-specific DexScreener-only path with the official public
DLMM `/pools` API. The connector filters configured token pairs, normalizes the
top-pool price, and emits TVL, 24-hour volume/fees, fee/TVL ratio, turnover,
dynamic fee, bin step, blacklist and pagination metrics as `defi_native_state`.
Added bilingual Python monitor/recorder/replay and a synchronized BTC response
study. Provenance: [Meteora DLMM pools API](https://docs.meteora.ag/api-reference/dlmm/pools/pools).
Partial pages remain observe-only; no swap, transaction, wallet, signing,
position or LP-PnL path is included.

## 2026-09-14 — Orca Whirlpool public API context

Upgraded the Orca source from DexScreener-only snapshots to the official
public Whirlpool API. The connector filters configured token-pair searches,
normalizes top-pool price orientation, and emits pool count, TVL, 24-hour
volume/fees, yield-over-TVL, top turnover, warning, adaptive-fee and cursor
metrics as `defi_native_state`. Added bilingual monitor/recorder/replay and a
BTC response study for warning/adaptive-fee versus ordinary states. Provenance:
[Orca API overview](https://docs.orca.so/api-reference/overview) and
[Whirlpools reference](https://docs.orca.so/api-reference/whirlpools). No swap,
transaction, wallet, signing, position or LP-PnL path is included.

## 2026-09-14 — Raydium native pool concentration

Added a read-only Raydium API v3 pool-info poller. Each configured mint pair
now emits page count/coverage, page TVL, 24-hour volume and fees, top-pool TVL
and volume shares, top price/fee rate, and page/top turnover as
`defi_native_state` metrics. Added bilingual Python monitor, recorder and
replay plus a BTC response recorder/replay for fragmented versus concentrated
high-turnover states. The source is the official [Raydium pool-by-mint API documentation](https://docs.raydium.io/api-reference/api-v3-endpoints/pools/get-pools-by-token-mint);
pagination remains explicit and no swap, transaction, wallet, signing or LP
PnL path is included.

## 2026-09-14 — Jupiter route-impact ladder

Closed the Jupiter DeFi data gap by adding optional `route_amounts` to each
configured quote pair. The Rust connector now preserves the primary quote and
emits size-suffixed `defi_native_state` metrics for route hops, raw
`priceImpactPct`, normalized route price, and output amount. Added the Python
`crypto_jupiter_route_impact_monitor.py`, recorder and replay under the DeFi
family, with bilingual documentation and catalog coverage. The implementation
is grounded in the official [Jupiter quote API documentation](https://developers.jup.ag/docs/swap/v1/get-quote),
which exposes `priceImpactPct` and `routePlan`; it remains read-only and does
not construct transactions, sign wallets, or execute swaps.

## 2026-09-14 — VPIN-proxy response replay

Added `crypto_vpin_response_replay.py` under microstructure. It builds fixed
quote-notional volume buckets, computes a rolling mean of absolute signed
taker imbalance, and compares high-VPIN-proxy buckets with normal buckets by
later absolute movement. The broad public [Bookmap order-flow/volume lead on
X](https://x.com/bookmap_pro/status/1945883967409819779) is not treated as a
performance claim; the construction is cross-checked with the primary [crypto
microstructure study](https://www.frontiersin.org/journals/blockchain/articles/10.3389/fbloc.2026.1811716/full).
The proxy keeps partial buckets, provider side semantics and no-execution
boundaries explicit.

## 2026-09-14 — event-time trade-imbalance-bar replay

Added `crypto_trade_imbalance_bar_replay.py` under microstructure. It closes
event bars at a fixed quote-notional threshold or trade-count cap, preserves
provider taker-side semantics, and compares strong signed imbalance with
balanced controls over later event bars. The research lead is the public
[delta/imbalance-bar discussion on X](https://x.com/quantbeckman/status/1931965694251253967),
cross-checked with the event-based [crypto microstructure study](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=6057134)
and [Binance's public futures market-data documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
The case remains a bounded representation/response study with no fill or
execution path.

## 2026-09-14 — cross-venue funding-spread response replay

Added `crypto_funding_spread_response_replay.py` under the carry family. It
annualizes each venue's historical funding with its point-in-time settlement
interval, rejects stale as-of values, and compares fixed-horizon absolute BTC
returns after extreme or shocked two-venue spreads with ordinary spread
windows. The research lead comes from the public [cross-venue funding-spread
discussion on X](https://x.com/leondoteth/status/2012127303850213817); funding
semantics are cross-checked with [Binance's funding-history documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)
and [OKX's funding-rate-history documentation](https://app.okx.com/docs-v5/zh/#rest-api-public-data-get-funding-rate-history).
It does not estimate carry cash flow, fills, hedges or execution.

## 2026-09-14 — options term-structure response replay

Added `crypto_options_term_structure_response_replay.py` under the options
family. It reuses the skew-response JSONL archive, joins near/far ATM-IV slope
states to synchronized BTC prices, and compares signed and absolute responses
after upward, inverted and flat states. Provenance: the existing [Deribit
options data guide](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/)
and the public [options-market brief on X](https://x.com/Gate_Launch/status/2063810805552845140).
Expiry roll, surface interpolation, option PnL, hedging and execution remain
outside the case.

## 2026-09-14 — triangular quote response replay

Added `crypto_triangular_arbitrage_response_recorder.py` and
`crypto_triangular_arbitrage_response_replay.py` under the carry family. The
recorder joins the existing BTCUSDT/ETHBTC/ETHUSDT quote-consistency snapshot
with a synchronized BTC quote; replay compares signed and absolute responses
after qualifying versus unqualified three-leg edges. Provenance: the public
[triangular-arbitrage example on X](https://x.com/AntCaveClub/status/1919359974456897634),
cross-checked with [Binance's official spot market-data API](https://developers.binance.com/en/docs/products/spot/rest-api)
and the peer-reviewed [triangular-arbitrage exploitability study](https://www.sciencedirect.com/science/article/pii/S154461232401537X).
No atomic routing, wallet or execution path is added.

## 2026-09-14 — directional USDC/USDT rotation replay

Added `crypto_stablecoin_rotation_response_replay.py` under the DeFi family.
It reuses the existing stablecoin-depeg JSONL archive, normalizes both
`USDCUSDT` and inverse `USDTUSDC` quotes into USDC priced in USDT, and tests
the falsifiable claim that a USDC discount aligns with positive BTC response
while a premium aligns with negative response. Provenance: the public [USDC
rotation discussion on X](https://x.com/xiaomustock/status/1916307721809721637),
cross-checked with the existing [Binance public market-data documentation](https://developers.binance.com/en/docs/products/spot/rest-api).
The output remains a directional event study and does not infer capital flows,
redemption, conversion or execution.

## 2026-09-14 — cross-venue order-book response replay

Added `crypto_cross_venue_orderbook_response_recorder.py` and
`crypto_cross_venue_orderbook_response_replay.py` under the carry family. The
recorder joins the existing timestamp-skew-filtered, target-notional VWAP edge
with a synchronized MarketBridge BTC quote; replay compares signed and
absolute responses after qualifying versus unqualified snapshots. Provenance:
the public [cross-venue funding-spread discussion on X](https://x.com/leondoteth/status/2012127303850213817),
[Binance's order-book API documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data),
and the [cross-exchange arbitrage-friction study](https://academic.oup.com/rof/article/28/4/1345?guestAccessKey=50540e27-1995-48e8-bb51-6b93b219d2ad).
No fill, inventory, routing or execution path is added.

## 2026-09-14 — options IV-skew response replay

Added `crypto_options_skew_response_recorder.py` and
`crypto_options_skew_response_replay.py` under the categorized options family.
The recorder joins the existing target-expiry put-wing-minus-call-wing
snapshot with a synchronized MarketBridge BTC quote; replay compares signed
and absolute fixed-record responses across downside-protection, upside-call and
balanced-wing states. Provenance: the public [options brief on X](https://x.com/Gate_Launch/status/2063810805552845140),
cross-checked with the existing [Deribit options data guide](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/).
Moneyness buckets are deliberately not treated as a universal 25-delta surface,
and no option PnL, hedge or execution path is added.

## 2026-09-14 — frozen on-chain transfer response replay

Added `crypto_onchain_transfer_response_recorder.py` and
`crypto_onchain_transfer_response_replay.py` under the categorized on-chain
family. The recorder freezes `/v1/onchain/transfers` beside a synchronized
MarketBridge BTC quote; replay deduplicates repeated provider rows using only
observable fields and compares rolling burst versus ordinary absolute BTC
responses. Provenance: the public [stablecoin-liquidity discussion on X](https://x.com/Cointelegraph/status/2029519994652942494),
the [BIS transfer-level study](https://www.bis.org/publications/working-paper-1359-anatomy-stablecoin-transactions),
and [Whale Alert's API schema](https://developer.whale-alert.io/api-account/documentation).
Transfer direction remains metadata, coverage is explicit, and no wallet or
execution path is added.

## 2026-09-14 — bull-call-spread response replay

Added `crypto_options_bull_call_spread_response_recorder.py` and
`crypto_options_bull_call_spread_response_replay.py` under the categorized
options family. The recorder joins the existing one-expiry, moneyness-selected
paper spread geometry with a synchronized MarketBridge BTC quote; replay
compares signed and absolute fixed-record responses after observable spread
states versus unvalidated snapshots. Provenance: the unverified [Laevitas
bull-call-spread flow observation on X](https://x.com/laevitas1/status/1985373005644476891),
cross-checked with [Deribit's public option-book documentation](https://docs.deribit.com/api-reference/market-data/public-get-order-book).
The output remains quote research evidence and adds no option PnL, hedge or
execution path.

## 2026-09-14 — DeFi pool-flow response replay

Added `crypto_defi_pool_flow_response_recorder.py` and
`crypto_defi_pool_flow_response_replay.py` under the categorized DeFi family.
The recorder joins the existing normalized `defi_native_state`/`dex_pool`
snapshot with a synchronized MarketBridge BTC quote; replay compares signed
and absolute fixed-record responses after pressure versus ordinary pool states.
The hypothesis is grounded in [Uniswap's explanation of pool liquidity and
price impact](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works)
and [swap execution](https://developers.uniswap.org/docs/get-started/concepts/traders/swaps).
Provider snapshots, sampling gaps and the no-route/no-wallet/no-execution
boundary remain explicit.

## 2026-09-14 — footprint pressure response replay

Added `crypto_footprint_response_recorder.py` and
`crypto_footprint_response_replay.py` under microstructure. The recorder joins
the existing bounded `/v1/market/footprint` state to a MarketBridge quote;
replay compares bid-pressure, ask-pressure and ordinary snapshots at a fixed
record-count horizon, reporting signed and absolute responses. Provenance: the
unverified [OI/flow confirmation discussion on X](https://x.com/xwinfinance/status/2023155692916646257).
No resting-book, ownership, fill or execution claim is added.

## 2026-09-14 — external BTC ETF-flow response replay

Added `crypto_etf_flow_response_replay.py` under macro. It parses a
Farside-style daily CSV in USD millions, aligns flow dates with MarketBridge
daily BTC candles, and compares large inflow, large outflow and ordinary-flow
response buckets. The external CSV boundary, missing rows, revisions, NAV
timing and no-execution limitation are explicit; no flow value is synthesized.
Provenance: [Farside's Bitcoin ETF flow table](https://farside.co.uk/btc/) and
the existing [Wintermute macro/crypto-liquidity research lead on X](https://x.com/wintermute_t/status/1985631560021000352).

The same round adds the optional Rust `farside_etf` connector. When enabled in
`aggregates.farside_etf`, it publishes the latest dated total as
`/v1/external/signals?sources=farside_etf` with source timestamp and USD-million
units. It is a latest-snapshot connector, not a fabricated historical backfill.

## 2026-09-14 — adaptive cross-asset paper-index replay

Added `crypto_adaptive_cross_asset_replay.py` under the universe family. It
keeps volatility-normalized signed scores for every asset, applies a caller-
visible gross cap, shrinks conflicting signals to a neutral paper index, and
compares the fixed-horizon result with an equal-weight basket. Provenance: the
public [RoboNet multi-asset strategy discussion on X](https://x.com/RoboNetHQ/status/2024893544520143012),
cross-checked with [CME's crypto diversification study](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html).
No Allora prediction, wallet, order or execution path is used.

## 2026-09-14 — matched-clock weekday/hour effect replay

Added `crypto_weekday_hour_effect_replay.py` under microstructure. The replay
compares a selected UTC weekday/hour (Tuesday 05:00 UTC by default) with other
weekdays at the same hour, reporting the event candle, next-hour bounce and a
fixed later response. It keeps missing bars, sample sufficiency, paper costs
and the no-execution boundary explicit. Provenance: the unverified [recurring
Tuesday 05:00 UTC BTC-selling discussion on X](https://x.com/Sherlockwhale/status/2041499514033320163),
cross-checked with [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

## 2026-09-14 — unsigned options gamma response study

Added `crypto_options_gamma_response_recorder.py` and
`crypto_options_gamma_response_replay.py` under the options family. The
recorder joins the existing bounded Deribit gamma-map observation to a
MarketBridge BTC quote; replay compares signed and absolute forward spot
responses after near-spot, single-strike gamma concentration versus other
states. Missing quotes are excluded, record-count horizons are explicit, and
the output never infers dealer gamma sign, option PnL, hedge demand or
execution. Provenance: the public [gamma-wall discussion on X](https://x.com/david_eng_mba/status/2042265877488533758),
cross-checked with [Deribit's public option-book documentation](https://docs.deribit.com/api-reference/market-data/public-get-order-book).

## 2026-09-14 — session VWAP-deviation reversion replay

Added `crypto_vwap_deviation_reversion_replay.py` under microstructure. It
resets at UTC midnight, computes volume-weighted typical-price VWAP and weighted
standard deviation from OHLCV, then records only a cross-back after the prior
close was outside a percentage/standard-deviation band. The fixed forward
window reports gross and paper-cost-adjusted directional responses and can
honestly remain negative. Provenance: the public [BTC VWAP mean-reversion
study](https://www.coinquant.ai/blog/vwap-strategy-backtest-on-bitcoin-intraday-mean-reversion-results)
and [VWAP-band construction reference](https://www.basischarts.com/indicators/vwap-bands).
No position, stop or execution model was added.

## 2026-09-14 — keyed social-signal response replay

Added `crypto_social_signal_response_recorder.py` and
`crypto_social_signal_response_replay.py` under the sentiment family. The
recorder freezes one configured LunarCrush or Santiment metric beside a
MarketBridge BTC quote; replay compares social-rise/social-fall changes with
later absolute movement at a fixed record-count horizon. It keeps the provider
metric name and scale, keyed coverage, model revisions, timestamp alignment and
the no-order boundary explicit. Provenance: [LunarCrush's public X post on
social activity and BTC highs](https://x.com/LunarCrush/status/1864875012051865680),
cross-checked against the [LunarCrush API field documentation](https://github.com/lunarcrush/api)
and [social-intelligence methodology](https://lunarcrush.com/faq).

## 2026-09-14 — Bollinger BandWidth squeeze replay

Added `crypto_bollinger_squeeze_replay.py` under the microstructure family.
It uses only candles known before the trigger: a trailing empirical BandWidth
quantile identifies a squeeze, the next close breaking the prior upper/lower
band defines the event, and a fixed forward candle window reports gross and
paper-cost-adjusted continuation. This is deliberately separate from the
existing realized-volatility/range breakout replay and omits ATR stops,
leverage, fills and execution. Provenance: [John Bollinger's BandWidth/Squeeze
rules](https://www.bollingerbands.com/bollinger-band-rules) and the unverified
[VWAP + Bollinger squeeze lead on X](https://x.com/instaclaws/status/2038363051213181035).

## 2026-09-14 — open-interest impulse response study

Added the carry-family Python `crypto_oi_impulse_response_recorder.py` and
`crypto_oi_impulse_response_replay.py`. The recorder freezes current perpetual
OI beside a MarketBridge quote; replay compares OI expansion, contraction and
ordinary snapshots with later absolute price movement at a fixed record-count
horizon. This turns the public claim that aggressive OI growth can provide fuel
for another liquidation wave into a non-directional, falsifiable risk-context
test. It keeps OI ownership, elapsed-time alignment, causality, fees and
execution outside the result. Provenance: [XWIN's public OI/liquidation-risk
discussion on X](https://x.com/xwinfinance/status/2023155692916646257),
cross-checked against [Binance's derivatives market-data documentation](https://developers.binance.com/zh-CN/docs/catalog/core-trading-derivatives-trading-coin-futures/api/rest-api/market-data).

## 2026-09-14 — short-squeeze fixed-record response replay

Added `crypto_short_squeeze_response_recorder.py` and
`crypto_short_squeeze_response_replay.py` under the microstructure family.
The recorder freezes the existing funding/OI/spot-perp-flow confluence score
beside a MarketBridge quote; replay compares score-qualified snapshots with
`observe_only` snapshots at a caller-selected record-count horizon. It keeps
the first-poll OI cold start, venue-side semantics, bounded history, paper-cost
sensitivity and the no-order boundary explicit. Provenance: the unverified
[public L2 imbalance plus funding-extreme perp lead on X](https://x.com/instaclaws/status/2038363051213181035).

## 2026-09-14 — aggregate crowding fixed-window response replay

Added a separate Python recorder/replay lifecycle for the public crowded-side
and liquidation-reversal narrative. The recorder freezes optional CoinGlass
funding/OI/long-short/liquidation context beside a MarketBridge quote; replay
uses a caller-selected record-count horizon and reports signed responses for
long crowding, short crowding, and liquidation-qualified subsets. The sign is
an explicit descriptive transform (negative next return for long crowding and
positive next return for short crowding), not a direction or execution rule.
Missing metrics, provider semantics, elapsed-time irregularity, paper costs and
no-order boundaries are preserved in output and the bilingual microstructure
guide. Provenance: [CryptoData's public liquidation-threshold discussion on
X](https://x.com/TheCryptoData/status/1948466627365769584).

## 2026-09-14 — altcoin breadth relative-response replay

Added `crypto_altcoin_breadth_replay.py` to separate participation breadth
from the existing cross-asset momentum ranking. For a caller-selected BTC and
altcoin set, the replay measures the fraction of alts whose trailing return
beats BTC, then reports the next equal-weight alt-basket versus BTC relative
response by low/neutral/high breadth state. It preserves exact timestamp
intersections, missing histories and a paper cost hurdle. The implementation is
an auditable proxy rather than BlockchainCenter's official market-cap-weighted
Top-50 Altcoin Season Index. Provenance: [BlockchainCenter's Altcoin Season
Index definition](https://www.blockchaincenter.net/altcoin-season-index/) and
the public [xWIN altcoin-index discussion on X](https://x.com/xwinfinance/status/1951412106345193606).

## 2026-09-14 — event-anchored VWAP replay

Added `crypto_anchored_vwap_replay.py` to separate event-anchored VWAP from
the existing session VWAP case. For every candle it chooses a swing low or high
only from a preceding lookback, computes typical-price OHLCV VWAP from that
anchor, requires a fresh reclaim/rejection and optional volume confirmation, and
measures the next fixed candle-index response. The implementation explicitly
does not claim an externally verified event timestamp, tick-level volume, or
execution edge. Provenance: the unverified [anchored VWAP discussion on
X](https://x.com/Jake__Wujastyk/status/1873917626638098894), cross-checked with
[Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

## 2026-09-14 — footprint imbalance persistence monitor

Added Python monitor/recorder/replay entrypoints for the existing
`/v1/market/footprint` endpoint. The case summarizes price-bin bid/ask delta,
stacked imbalance and pressure direction, then requires consecutive snapshots
before reporting persistence. It explicitly retains the rolling in-memory
trade-buffer, binning and taker-side limitations; no forward-return, order-book
or execution claim is added. Provenance: the unverified [public OI/flow
confirmation discussion on X](https://x.com/xwinfinance/status/2023155692916646257).

## 2026-09-14 — volume-profile low-volume-node replay

Added the microstructure-family `crypto_volume_profile_breakout_replay.py`.
It builds a rolling value area from historical OHLCV, requires a close to leave
that area into a low-volume bin with volume confirmation, and measures a fixed
forward horizon after an explicit paper-cost hurdle. The implementation labels
its important approximation: candle volume is assigned to typical price because
the current history interface does not expose tick-level volume-at-price.
Provenance: the public [compression-to-expansion / LVN lead on X](https://x.com/Stoiiic/status/1796078958674628714)
and the research [Liquidity-Driven Breakout Reliability paper](https://papers.ssrn.com/sol3/Delivery.cfm/5962358.pdf?abstractid=5962358&mirid=1).
No execution path was added.

## 2026-09-14 — session confluence historical replay

Added the microstructure-family `crypto_session_momentum_replay.py` to turn
the existing current snapshot filter into a bounded historical event study. It
uses a caller-selected timezone/session window, session typical-price VWAP,
EMA(9/21), MACD acceleration and volume confirmation, then measures the next
fixed candle horizon with an explicit paper-cost hurdle. Provenance: the
unverified [15-minute VWAP/EMA/MACD/volume discussion on X](https://x.com/Gustafssonkotte/status/2030566353178882122)
and [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).
No execution path was added.

## 2026-09-14 — stablecoin depeg-risk event study

Added the DeFi-family Python stablecoin monitor/recorder/replay. The monitor
normalizes selected CEX or DEX-pool stablecoin pairs, measures deviation from
one-for-one and top-of-book spread stress, and optionally keeps a BTCUSDT quote.
The replay compares later absolute BTC movement after stressed snapshots with
ordinary snapshots. It deliberately does not infer reserves, redemptions,
solvency or executable mean reversion. Provenance: the unverified [DEWS-style
early-warning discussion on X](https://x.com/crazydnekana/status/2030633787462242588),
the peer-reviewed [Tether depegging and crypto returns study](https://doi.org/10.1111/acfi.70201),
and [Detecting Depegs](https://arxiv.org/abs/2306.10612).

## 2026-09-14 — quarter-hour order-flow replay

Added the microstructure-family Python replay for a phase-aligned order-flow
hypothesis. It selects UTC quarter-hour openings, measures a short signed
taker-flow window from `/v1/history/trades`, joins the fixed-horizon return from
1-minute perp candles, and reports gross versus paper-cost-adjusted alignment.
The case keeps public-history truncation, provider side semantics and the
non-causal clock-phase interpretation explicit; it has no execution path.
Provenance: the unverified [public X order-book/funding lead](https://x.com/instaclaws/status/2038363051213181035),
the primary [Quarter-Hour Effect research paper](https://arxiv.org/abs/2607.09426),
and [Binance's official funding/order-book documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).

## 2026-09-14 — triangular quote-consistency replay

Added the carry-family Python monitor/recorder/replay for a bounded
single-venue triangular conversion hypothesis. It reads synchronized spot
top-of-book quotes for BTCUSDT, ETHBTC and ETHUSDT, evaluates both USDT cycle
directions, subtracts a paper per-leg cost and requires consecutive qualifying
snapshots in replay. The output is deliberately a quote-consistency candidate,
not an executable arbitrage claim: depth, atomicity, queue position, latency,
inventory, fees and partial fills remain missing. Provenance: [Binance's
official spot market-data API documentation](https://developers.binance.com/en/docs/products/spot/rest-api)
and the peer-reviewed [Wish or reality? On the exploitability of triangular
arbitrage in cryptocurrency markets](https://www.sciencedirect.com/science/article/pii/S154461232401537X).

## 2026-09-14 — cross-venue order-book paper-edge replay

Added the carry-family Python monitor/recorder/replay for synchronized
cross-venue order-book edges. It computes target-notional VWAP on the buy ask
and sell bid sides, rejects timestamp skew, subtracts an explicit paper cost,
and requires consecutive qualifying snapshots before reporting persistence.
This closes the gap between candle-only price fragmentation and executable-depth
evidence while preserving the no-order boundary: inventory, transfer/settlement
latency, queue position, fees and venue risk remain explicit limitations.
Provenance: [Binance's public order-book documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data)
and the [cross-exchange arbitrage-friction study](https://academic.oup.com/rof/article/28/4/1345?guestAccessKey=50540e27-1995-48e8-bb51-6b93b219d2ad).

## 2026-09-14 — preserve CryptoPanic news instances and add attention replay

Fixed the optional CryptoPanic connector so each returned post uses its
canonical URL as `source_instance`; the in-memory external-signal snapshot no
longer overwrites the bounded feed's ten `news_item` rows under one key. Added
a regression test for distinct URLs and RFC3339 `published_at` to
`source_time_ms` mapping. Added Python sentiment monitor/recorder/
replay cases that classify high-absolute-vote-score bursts and compare their
subsequent absolute price movement with ordinary aligned windows. This is a
non-directional research diagnostic with no allocation, wallet or execution
path. Provenance: [CryptoPanic's official integration guide](https://cryptopanic.com/guides/how-to-integrate-the-cryptopanic-api)
and the peer-reviewed [crypto news-headline impact study](https://www.sciencedirect.com/science/article/pii/S0264999323002092).

## 2026-09-14 — Fear & Greed sentiment-extremes replay

Added the Python-first `examples/crypto/sentiment/` family. The monitor reads
the existing `fear_greed` external signal and a selected MarketBridge price
snapshot; the recorder freezes both to JSONL; the replay compares fixed-record-
horizon forward returns after extreme fear/greed states with aligned ordinary
windows. Missing prices and minimum sample counts remain explicit, and paper
cost is only a sensitivity input. There is no allocation, wallet or execution
path. Added bilingual index/docs and deterministic tests. Provenance is
[Alternative.me's official Crypto Fear & Greed API description](https://alternative.me/crypto/fear-and-greed-index/)
and the unverified [BitcoinFear public X extreme-value post](https://x.com/BitcoinFear/status/2043554800046940376).

## 2026-09-14 — derivatives sentiment persistence recorder/replay

Added Python `crypto_derivatives_sentiment_recorder.py` and
`crypto_derivatives_sentiment_replay.py` under the microstructure family. The
recorder archives the existing optional CoinGlass aggregate signal surface as
JSONL; the replay sorts captures by time and requires consecutive long/short
crowding states before reporting a persistence candidate. Aggregate ratios are
kept as provider context rather than position ownership, and the case contains
no price forecast, allocation, wallet or execution path. Added bilingual index
entries and deterministic tests. Provenance remains the unverified public
[CoinGlass positioning discussion on X](https://x.com/ImCryptOpus/status/1949195275903410571).

## 2026-09-14 — aggregate derivatives sentiment context monitor

Added `crypto_derivatives_sentiment_monitor.py` for the existing optional
CoinGlass external-signal surface. It groups funding, OI, long/short ratio,
basis, liquidation and options OI metrics into an explicit crowding context;
missing API keys or metrics stay observe-only. It does not infer trader
ownership or add a historical or execution path. Added bilingual microstructure
docs and deterministic tests.

## 2026-09-14 — crypto macro context monitor

Added the categorized `examples/crypto/macro/` family. The Python monitor
joins configured DXY, VIX and US10Y quote snapshots with one crypto funding row
and emits explicit missing-data/elevated-volatility context. It is not a
historical factor replay or return forecast; VIX is an SPX option-implied
reference and all macro snapshots remain provider/configuration dependent.
Added bilingual provenance docs and deterministic tests.

## 2026-09-14 — Python aggregate market-regime context monitor

Added `crypto_market_regime_monitor.py` under the universe family for the
existing `/v1/research/market-regime` feature bundle. It exposes fragmented,
high-volatility, leveraged and normal context to Python researchers while
keeping the current-only/non-point-in-time limitation explicit. The monitor
does not select, size or execute a strategy; added bilingual docs and
deterministic tests.

## 2026-09-14 — universe stale-quote risk guard

Added `crypto_universe_delist_risk_monitor.py` for the existing
`/v1/universe/delist-risk` API. It reports missing and stale current quotes for
historically seen markets before a universe strategy treats them as candidates;
it deliberately labels the result as data-quality review, not a delisting
forecast, and has no automatic exclusion or execution side effect. Added
bilingual universe docs and deterministic tests.

## 2026-09-14 — strategy catalog consistency guard

Added `examples/test_strategy_catalog.py` to keep the Python-first strategy
library maintainable as new cases arrive. CI now verifies every categorized
family has an English/中文 README and command section, every categorized Python
entrypoint is referenced by the root or family index, and no Rust strategy file
appears under `examples/`. This is a documentation/organization guard only; it
does not broaden the execution boundary.

## 2026-09-14 — cross-venue same-asset price-gap replay

Added `crypto_cross_venue_price_gap_replay.py` to separate same-asset venue
fragmentation from same-venue basis, cross-venue funding and two-asset pairs.
It uses exact timestamp candle intersections, a frozen trailing log-gap mean,
z-score signals and an optional paper hurdle. It deliberately does not call a
gap an arbitrage opportunity: fills, inventory, transfers, fees, latency and
venue risk are missing. Added bilingual carry docs and deterministic tests,
with provenance from [Trading and Arbitrage in Cryptocurrency Markets](https://www.sciencedirect.com/science/article/pii/S0304405X19301746)
and [Arbitrage across different Bitcoin exchange venues](https://onlinelibrary.wiley.com/doi/10.1111/acfi.13102).

## 2026-09-14 — CVD divergence reversal replay

Added `crypto_cvd_divergence_replay.py` to separate historical price/taker-flow
divergence from the existing snapshot confirmation and breakout cases. It
joins bounded `/v1/history/trades` with candles, requires a material price move
and opposite signed-flow ratio, and measures the next-window aligned reversal
with an optional paper hurdle. Added bilingual microstructure docs and
deterministic tests; single-venue coverage, trade-side semantics and missing
history remain explicit. CVD semantics are referenced from [a public CVD
indicator explanation](https://mindpillar.com/cvd/), not treated as a
performance claim.

## 2026-09-14 — crypto pair mean-reversion replay

Added `crypto_pairs_mean_reversion_replay.py` to cover relative-value research
that was absent from the momentum and funding cross-sectional cases. It uses
exact timestamp candle intersections, a caller-supplied fixed hedge ratio, and
freezes trailing spread mean/standard deviation before measuring future
convergence. Added bilingual universe documentation and deterministic tests;
the case does not claim cointegration, market neutrality, paired fills, PnL or
execution. Provenance is the peer-reviewed [crypto pairs-trading study](https://ieeexplore.ieee.org/document/9200323/)
and the public [Pairs Trading in Crypto paper](https://papers.ssrn.com/sol3/Delivery.cfm/6188418.pdf?abstractid=6188418&mirid=1&type=2).

## 2026-09-14 — cross-sectional funding dispersion replay

Added `crypto_funding_cross_section_replay.py` to separate cross-asset funding
dispersion from the existing same-symbol regime and cross-venue convergence
cases. It uses fresh point-in-time funding observations, exact price timestamp
intersections, low/high funding groups and an explicit optional paper hurdle.
Coverage, interval freshness and missing intersections remain visible; there
is no allocation, hedge, funding-income, leverage or order path. Added bilingual
index/docs and deterministic tests. Provenance is the public [funding
differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
plus [Binance's funding-history API documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).

## 2026-09-14 — bounded on-chain transfer burst replay

Added the categorized `examples/crypto/onchain/` case for a public large-
transfer liquidity lead. It compares rolling transfer USD bursts from
`/v1/onchain/transfers` with ordinary forward absolute candle moves from
`/v1/history/candles`; it preserves direction, asset, chain and source only as
metadata. The implementation explicitly rejects exchange-net-flow, stablecoin
supply and directional claims because the cache is bounded and transfer events
may be bundled or internal. Added bilingual provenance, a deterministic test,
and no wallet or execution path.

## 2026-09-14 — DeFi pool flow and liquidity-pressure examples

Added the categorized `examples/crypto/defi/` family. The monitor joins
MarketBridge's `defi_native_state` signals with `dex_pool` quotes and classifies
one-hour swap-volume-to-liquidity pressure; the recorder and replay require
consecutive snapshots before reporting a persistent pool-pressure candidate.
This uses existing normalized data and does not add routing, gas, LP PnL,
wallet signing or swap execution. Added bilingual documentation and
deterministic tests, with the hypothesis grounded in [Uniswap's explanation of
pool liquidity and price impact](https://developers.uniswap.org/docs/get-started/concepts/how-uniswap-works).

## 2026-09-14 — options IV term-structure persistence replay

Added `crypto_options_term_structure_replay.py` under the categorized options
family. It replays the near/far ATM-IV slope already captured by the skew
recorder, classifies upward/inverted/flat states, and requires a configurable
consecutive run before reporting a research candidate. This closes the gap
between the existing term-structure snapshot and a reproducible temporal test;
it does not model a calendar spread, option PnL, expiry roll, hedge, fees or
execution. Added a bilingual guide, categorized launcher and deterministic
tests. The definition is cross-checked against [Deribit Insights' options data
guide](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/).

## 2026-09-14 — funding convergence coverage propagation

Updated the cross-venue funding convergence replay to preserve each venue's
historical candle `coverage_detail` and add provider coverage statuses to its
evidence. Gross/after-cost differential statistics are unchanged; only the
completeness audit is stronger.

## 2026-09-14 — propagate history coverage into strategy evidence

Updated the funding/OI replay, volatility-breakout replay and options VRP
monitor to forward MarketBridge `coverage_detail` into their JSON evidence.
Funding, OI, price-candle and realized-volatility windows now expose provider
page truncation at the strategy layer instead of silently treating a bounded
response as complete. Added bilingual guidance; no signal or execution logic
changed.

## 2026-09-14 — historical candle coverage metadata

Extended `GET /v1/history/candles` with the same bounded `coverage_detail`
contract as historical OI, liquidations and trades. Candle consumers now see
requested/covered timestamps, returned rows, page limit and possible provider
truncation; funding schedules remain a separate point-in-time interval artifact.
Added route-level tests and kept the no-completeness-claim boundary explicit.

## 2026-09-14 — open-interest history coverage metadata

Extended `GET /v1/history/open-interest` with bounded `coverage` and
`coverage_detail`: requested/covered timestamps, returned rows, page limit and
an explicit truncation status. This mirrors the liquidation-history contract so
Python regime replays can distinguish sparse OI alignment from a genuinely
empty feed. Added route-level tests for timestamp bounds and non-completeness.

## 2026-09-14 — point-in-time positioning regime replay

Added a carry-family regime matrix replay that joins funding history, aggregate
open interest and perp candles at explicit timestamps. It labels each usable
observation by price trend, OI change and funding sign, then reports forward
return distributions and continuation hit rates by regime. Missing inputs are a
separate state; OI is never interpreted as long/short ownership. Added a
categorized launcher, bilingual guidance, public-source provenance and
deterministic timestamp/coverage tests. No order, hedge or wallet path was
added.

Provenance: [public OI/funding/price context brief on X](https://x.com/ImCryptOpus/status/1949195275903410571),
[Binance's official open-interest history documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info),
and MarketBridge's normalized historical interfaces.

## 2026-09-14 — cost-aware volatility breakout replay

Extended the compression-to-expansion breakout replay with a fixed
`--roundtrip-cost-bps` paper hurdle and `--min-cost-adjusted-edge-bps`
qualification threshold. Events now preserve gross and cost-adjusted aligned
returns, hit rates and an explicit observe-only verdict when the hurdle or
sample count is not met. The hurdle is a transparent sensitivity parameter,
not a venue fee, fill, funding or latency model. Added deterministic regression
coverage and bilingual usage guidance.

## 2026-09-14 — universe candidate persistence recorder

Added a recorder/replay lifecycle for the bounded universe opportunity scanner.
The recorder freezes joined volume/realized-volatility/funding candidate sets;
the replay reports top-k candidate persistence, symbol snapshot fractions and
empty-set coverage. Missing joins remain explicit, and persistence never
becomes an allocation, sizing or execution decision. Added categorized Python
launchers, bilingual usage and deterministic tests.

## 2026-09-14 — after-cost funding convergence sensitivity

Extended the cross-venue funding convergence replay with explicit
`--paper-cost-bps-per-hour` and `--min-net-spread-bps-per-hour` inputs. Each
aligned observation now preserves gross and net hourly differentials; the
candidate verdict uses the after-cost fraction while the gross statistics stay
visible. The hurdle is deliberately a paper sensitivity parameter, not a venue
fee, borrow, margin, slippage or hedge-fill model. Added deterministic tests
for net spread calculation and stale-venue exclusion, plus bilingual guidance.

Provenance: [public cross-venue funding spread discussion on X](https://x.com/leondoteth/status/2012127303850213817),
treated as an unverified research lead.

## 2026-09-14 — liquidity-stress recorder and persistence replay

Added the missing temporal lifecycle for the microstructure liquidity-stress
observer. The recorder archives target-size impact, spread, EWMA volatility,
state and upstream evidence; the replay counts only snapshots with all three
inputs available and requires a consecutive `liquidity_stress` run before
reporting a candidate. Missing depth/candles remain outside coverage. Added
categorized launchers, bilingual usage and deterministic tests; no routing,
order, hedge or sizing path was added.

## 2026-09-14 — options VRP recorder and persistence replay

Completed the options-family VRP lifecycle with a JSONL recorder and replay.
The monitor now retains the latest spot close for auditability; the recorder
freezes ATM IV, annualized perp RV, expiry identity and state, while the replay
tests whether an implied-volatility-premium state persists for the same expiry.
Missing IV/RV, rolling expiry identity, maturity mismatch and the absence of a
delta-hedge/PnL model remain explicit. Added bilingual guidance and
deterministic tests; no option order, hedge or wallet path was added.

Provenance: [public IV-minus-RV discussion on X](https://x.com/isellpremium/status/2072350364385349678)
and the [Bitcoin-options risk-premia paper](https://papers.ssrn.com/sol3/Delivery.cfm/98257442-0b56-4c20-8b8f-c91befac0b1b-MECA.pdf?abstractid=6771170).

## 2026-09-14 — observed liquidation price-cluster replay

Added a microstructure replay inspired by public liquidation-heatmap
discussions. It groups the observed rows from `/v1/history/liquidations` into
relative price bands, requires a configurable total notional and dominant-band
share, and compares the next fixed absolute price move with ordinary candle
windows. The implementation explicitly does not infer untouched liquidation
levels, leverage distributions, long/short truth or a directional trade. Added
a categorized launcher, bilingual documentation, provenance links and
deterministic tests for band concentration, cooldown and observe-only gating.

Provenance: [CoinGlass's public liquidation-heatmap post on X](https://x.com/coinglass_com/status/1930154005491282291)
and [Glassnode's liquidation-heatmap research](https://research.glassnode.com/liquidation-heatmaps/).
These are research leads; MarketBridge validates only the observable executed
liquidation-print subset.

## 2026-09-14 — persistent funding-regime replay

Added a carry-family funding-only replay that groups consecutive extreme
funding observations, preserves known schedule gaps, and measures whether the
next fixed perp-price window moves against the crowded-side proxy. It reports
run-level forward returns, hit rates, source counts and an explicit
`observe only` verdict; it does not infer positions, funding income, hedge PnL,
fills or orders. Added categorized launchers for the replay plus the existing
funding curve/extremes utilities, bilingual carry guidance and deterministic
tests for neutral breaks, schedule gaps and expected-direction scoring.

Provenance: [public funding-rate strategy explanation on Kraken](https://www.kraken.com/learn/futures-trading-funding-rate-strategy),
cross-checked with MarketBridge's provider schedule fields. The source is a
research lead, not a performance claim.

## 2026-09-14 — spot/perp depth-gap persistence replay

Extended the spot/perpetual target-size depth observer with a JSONL recorder and
persistence replay. The replay requires both a configurable advantage fraction
and a consecutive run before reporting a persistent perp-depth advantage;
missing sides and invalid target-size metrics remain outside the denominator.
Added categorized launchers, bilingual commands and deterministic tests for
fraction/run gating and missing-depth behavior. This remains a descriptive
execution-risk study, not a routing or hedge instruction.

## 2026-09-14 — spot/perp target-size depth gap monitor

Added a microstructure observer for same-venue spot/perpetual depth asymmetry.
It requests explicit spot and perp books plus basis context, computes target-size
depth and worst-side impact for both markets, and reports a perp/spot advantage
only when configurable depth and impact thresholds are both met. Missing sides,
unsynchronized snapshots and basis gaps remain visible; the monitor does not
route orders or infer hedge feasibility. Added a categorized launcher, bilingual
usage guidance and deterministic tests for material and missing gaps.

Provenance: [public spot/perp depth-gap discussion on X](https://x.com/ciaobelindazhou/status/2031929849850273955),
treated as an execution-risk research lead rather than a trading claim.

## 2026-09-14 — chronological holdout replay

Added `crypto_volatility_adjusted_momentum_walkforward.py` to separate
in-sample observations from a later chronological holdout for one fixed
parameter set. Test features may use only pre-split warm-up bars plus current
post-split data; test observations are never used for parameter selection. The
result includes train/test sample counts, cost-adjusted edges and explicit
`observe_only` behavior when a split is too short. Added a categorized launcher,
bilingual guidance and deterministic boundary tests. This is a paper research
diagnostic, not a claim of persistent alpha or a live execution path.

## 2026-09-14 — cost-aware paper hurdle for volatility momentum

Extended the volatility-adjusted replay and parameter sweep with an explicit
`--roundtrip-cost-bps` paper hurdle. Each observation now retains gross edge,
paper cost and cost-adjusted edge; candidate qualification and grid diagnostics
use the cost-adjusted mean while preserving the gross comparison. The hurdle is
deliberately a transparent relative sensitivity, not a venue fee, fill, queue,
capacity or paper-ledger claim. Added tests proving costs reduce reported edge
without changing the gross sample and updated bilingual usage guidance.

## 2026-09-14 — bounded volatility-adjusted momentum parameter sweep

Added a universe-family grid runner that fetches each symbol's historical
candles once and evaluates a bounded set of lookback, volatility and forward
horizon windows. It reports every row's sample count, edge, hit rate and
evidence, but labels the highest in-sample row as descriptive only and warns
that time-held-out, cost-aware validation is still required. Added a
categorized launcher, bilingual documentation and deterministic tests for grid
parsing, combination coverage and selection warnings.

## 2026-09-14 — categorized launchers completed for remaining Python cases

Added categorized Python launchers for the remaining maintained examples:
flow/book confirmation, the microstructure monitor, volatility-breakout
replay, session filter, and options-skew recorder/replay. The root
implementations remain the single source of logic; the launchers only put each
case in its research family and preserve the Python-only boundary. Updated the
microstructure/options bilingual guides and smoke-tested every new launcher
with `--help`.

## 2026-09-14 — rolling liquidation-burst replay

Added a microstructure replay for the public “large liquidation threshold may
precede a local move” narrative. It aggregates bounded public liquidation
notional over a rolling window, applies a cooldown so one burst is not counted
on every candle, and compares its forward absolute return with ordinary candle
windows. Side labels stay metadata only; no long/short inference, directional
trade, fill, fee, slippage or position-sizing model was added. Coverage status,
missing history and insufficient observations remain visible. Added a
categorized launcher, bilingual documentation and deterministic tests for
event filtering, cooldown, forward movement and the observe-only path.

Provenance: [CryptoData liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584),
treated as an unverified lead rather than a performance claim.

## 2026-09-14 — volatility-adjusted cross-asset momentum replay

Added a universe-family replay that ranks assets by trailing return divided by
per-bar realized volatility, then compares the selected basket with an
equal-weight benchmark over the next fixed horizon. It reuses exact timestamp
intersections, excludes zero-volatility assets instead of manufacturing an
infinite score, preserves missing history, and explicitly excludes fees,
funding, borrow, slippage, turnover, leverage and allocation execution. Added
a categorized launcher, bilingual universe documentation and deterministic
tests for volatility calculation, ranking evidence and insufficient forward
windows.

Provenance: [RoboNet's multi-asset strategy discussion on X](https://x.com/RoboNetHQ/status/2024893544520143012)
and [CME's crypto diversification study](https://www.cmegroup.com/articles/2025/diversifying-crypto-portfolios-with-xrp-and-sol.html).

## 2026-09-14 — basis contraction recorder and replay

Added a carry-family recorder/replay pair that preserves spot/perpetual basis
snapshots and funding context in JSONL, then tests the narrow hypothesis that a
same-venue basis observation beyond a trailing z-score threshold contracts over
the next fixed number of snapshots. The replay keeps exchange and symbol
identity, rejects insufficient history, reports contraction frequency rather
than PnL, and excludes hedge fills, borrow, fees, funding transfers, margin and
slippage. Added categorized launchers, bilingual carry documentation and
deterministic coverage for identity filtering, multi-level history and the
insufficient-sample verdict.

Provenance: [CryptoCred basis-trade discussion on X](https://x.com/CryptoCred/status/1777720296297975952)
and [CME-versus-spot basis example](https://x.com/0xscarlettw/status/1944584946670276938).

## 2026-09-14 — liquidity stress case and strategy-scoped requests

Added the Python-first `liquidity_stress` observer under
`examples/crypto/microstructure/`. It measures target-notional executable
book impact, quoted spread and non-annualized short-horizon EWMA volatility,
and reports `liquidity_stress` only when at least two components are elevated.
Missing depth or candles stays visible; the case has no direction, routing,
position-sizing or execution path. The shared runner now requests only the
endpoints required by the selected strategy (`/v1/market/order-books` and
`/v1/history/candles` for this case) instead of fetching the full core bundle
on every invocation. This makes a command's data boundary inspectable and
keeps the Rust server's background ingestion separate from Python scoring.

Provenance: [Pine Analytics / FlyingTulip execution-aware risk discussion on X](https://x.com/PineAnalytics/status/1974474638093590994).
The post is a research lead, not a performance claim. Added deterministic tests
for multi-level impact, EWMA missing-window behavior and the two-of-three gate.

## 2026-09-14 — Python-only categorized strategy examples

Migrated the four legacy Rust example monitors (squeeze, exhaustion, basis
carry and liquidation reversal) to Python launchers backed by the shared
`python_strategy_runner.py`. Removed Rust strategy files from `examples/` and
added bilingual category guides under `examples/crypto/` for carry,
microstructure, options/volatility and universe research. The categorized
launchers preserve simple commands while keeping one implementation, and the
server remains the Rust data/infrastructure process. Live smoke coverage ran
all four Python entrypoints for two iterations against Binance/OKX public data;
all returned structured `research_only_no_orders` observations. CI now guards
the Python-only invariant so Rust strategy files cannot silently return.

## 2026-09-14 — unsigned options gamma map and persistence replay

Added a Python-first gamma-map case for the public “gamma wall / gamma flip”
narrative. The observer aggregates `gamma`, `open_interest`, `strike` and
`underlying_price` into a relative unsigned gamma mass. A live check showed
Deribit’s summary cache does not always carry greeks, so the observer now uses
the existing `/options/deribit/book` route for bounded, explicit enrichment and
reports fetched/unfetched coverage instead of silently treating missing gamma
as zero. It identifies near-spot concentration and dominant strikes, but does
not infer dealer long/short gamma from public OI. Added a JSONL recorder/replay
pair and `options_gamma` runner mode so persistence can be measured before
anyone studies a realized-volatility response. This is a market-structure
observation, not a directional signal, hedge ratio or order path.

Provenance: [public BTC gamma-wall discussion on X](https://x.com/david_eng_mba/status/2042265877488533758); field semantics follow [Deribit public option-book documentation](https://docs.deribit.com/api-reference/market-data/public-get-order-book).

## 2026-09-14 — cross-asset momentum replay

Converted the public adaptive BTC/ETH/SOL perp-vault narrative into a
falsifiable, read-only case: at a fixed rebalance cadence, the strongest
trailing-return assets should beat an equal-weight basket over the next fixed
horizon. Added `examples/crypto_cross_asset_momentum_replay.py` and a
`cross_asset_momentum` mode in the Python strategy runner. The replay joins
only exact timestamps from `/v1/history/candles`, reports the selected basket,
forward edge and hit rate, and keeps missing assets visible. It deliberately
excludes fees, funding, borrow, slippage, weight drift, leverage and execution;
this is a research hypothesis, not a portfolio allocator or order path.

Provenance: [RoboNet adaptive horizon-aligned perp strategy discussion](https://x.com/RoboNetHQ/status/2024893544520143012).

## 2026-09-12 — weighted public-provider quota controls

Added optional `aggregates.provider_quotas` shared windows for custom public
HTTP sources. Each source can declare a named group and positive request weight;
configuration rejects undeclared groups, duplicate groups, zero weights and
weights exceeding the group capacity. Requests reserve the shared quota before
dispatch and wait for a new window on exhaustion, while preserving independent
origin pacing and Retry-After cooldowns. This is local protective pacing, not a
claim about provider account/IP policy or a bypass of service limits.
Added `/v1/system/provider-quotas` for read-only local window, consumed and
remaining-weight inspection; it deliberately does not claim upstream account
limits. Validation: strict all-target/all-feature Clippy and 311 Rust tests
passed locally. The isolated authenticated HTTP acceptance script now also
checks the default empty quota response and the integration-context contract,
then completed its existing replay, storage, reload and restart recovery checks.

## 2026-09-12 — scanner reset event semantics

Configuration changes and scanner restarts now persist a `scanner_reset` event
with an empty qualified set, so disabling scans does not leave cursor consumers
with a stale positive state. Added fresh-book/complete-cost qualification coverage;
unknown fees must not qualify. Existing missing-data coverage distinguishes
administrative reset events from false opportunity alerts.
Validation: strict all-target/all-feature Clippy and 307 Rust tests passed locally.

## 2026-09-12 — lockfile advisory maintenance

The feature push exposed default-branch Dependabot alert 9 for
[GHSA-4w2j-m93h-cj5j](https://github.com/advisories/GHSA-4w2j-m93h-cj5j).
Updated only `quinn-proto` 0.11.14 to the advisory's patched 0.11.15 in Cargo.lock.
The default and `--target all` dependency trees did not activate quinn-proto in
this build; this is lockfile hygiene, not a claim that the current HTTP runtime
had a demonstrated remotely reachable exploit. The default-branch alert will not
necessarily close while the fix remains only on a research feature branch.

## 2026-09-12 — publication identity audit

GitHub rejected the first feature-branch push with GH007 (private commit email).
The six unpublished research commits created during this work were rewritten to
the account's public noreply identity; Git tree equality was checked before/after.
No source content changed and no account privacy protection was disabled.
The obsolete local-only backup ref was removed after the public identity check.
Current milestone IDs: `4f25e3a` clocks, `62670bb` evidence/replay, `75b0435` paper,
`ce98eec` batch/live repair, `f0efe84` workspace, `90f7b1d` continuous research/manual.
This records commit identity maintenance, not additional test or release evidence.

## 2026-09-12 — continuous research workflows and usage manual

- Added background cached-book scanner, persisted transition alerts, validated
  immutable control revisions, file hot reload and visible last-good/error state.
- Added announcement ingestion/import and descriptive windows; allocated spot
  portfolios with reversible routes/exit gates; streaming whole-journal replay.
- Added embedded `/workbench`, asynchronous HTTP SDK with bounded concurrency,
  cancellation and durable cursor polling, and a complete Chinese usage series.
- End-to-end archive equality caught default JSON float parsing changing the last
  bits of `48002.200000000004`; enabled `float_roundtrip` and retained exact
  equality in regression/integration tests rather than loosening tolerances.
- Local Windows/MSVC: 306 Rust tests passed; all-target/all-feature strict Clippy,
  build, 13 Python contract tests and authenticated HTTP smoke passed. The HTTP
  suite verifies all nine archived models, exact archive retrieval, async real
  HTTP, valid/invalid file reload, and forced-process-restart persistence.
- Browser validation: real local page loaded, connected, ran/archived a synthetic
  experiment, loaded/stopped scanner configuration; screenshot rendered and
  captured browser warning/error log was empty. This used the in-app browser
  because the browser validation command was unavailable, not a mocked page.
- Public-source observation ending 2026-09-12 06:46:19 UTC: 60.16 seconds,
  12 samples, 11 with both Binance/OKX books and 11 changing pairs, zero HTTP errors,
  peak sampled working set 30,355,456 bytes. This was a dirty development build,
  binary SHA256 `2743215E5001A83195FB9EA5A5FE51579ABCC56C0329FCA60643B11ECC63BEB6`;
  retained local prefix `examples/out/soak-3b95807ade6540588f15da9f37d7c9b2`.
- No order, wallet, signing or private trading API added. No 72-hour, universal
  live-venue, final remote-CI, release-package or production acceptance claimed.
- Release packaging now includes research configs, SDK, scripts, examples,
  revision and archive checksums. License text is a separate owner decision:
  current README badge says MIT but this checkout contains no LICENSE file.

## 2026-09-12 — durable workspace and market-specific reference models

Added immutable SQLite research documents with CRC, versioned asset relationships,
ordered bounded dataset chunks, cursor replay and archived successful/failed runs.
Basis, unit-premium and interval-normalized funding models remain reference-only.
Validation: strict all-target/all-feature Clippy; 299 Rust tests; build;
`scripts/Test-ResearchApi.ps1` including registry, dataset, failed-run archive and
integrity assertions passed. Commit: `f0efe84` (original local ID `7d6bb98`). Local validation, not remote CI.

## 2026-09-12 — Batch screening and real-source depth repair

- Added bounded offline/cached-live candidate screening (64 routes), common
  decision cutoffs, after-cost ranking and visible per-candidate errors. Alternative
  sizes/shared liquidity are not added into a fabricated total profit.
- Added HTTP/CLI/Python interfaces, opt-in public-source config/example, bounded
  observation script and explicit candidate release checklist.
- First public observation (05:49–05:50 UTC) found no Binance book: the old
  spot-depth parser incorrectly required payload `s`. Fixed combined-stream
  identity, conflicting symbols, malformed levels and event-time preservation.
- Final local Windows/MSVC evidence: **293 Rust tests passed**; strict all-target/
  all-feature clippy, build, format/diff checks passed; **7 Python tests passed**.
- Authenticated HTTP smoke passed, including batch ranking, missing-live-book
  rejection and CLI/API agreement for batch/paper inputs.
- Post-fix public observation ending **2026-09-12 05:55:32 UTC**: 20 samples over
  20.42 seconds; 19 samples contained both books, with 19 distinct observations
  per venue. Last result had only the intentionally unverified relationship and
  unknown-cost reasons. No ranked opportunity or order was produced.
- Post-fix logs contained zero JSON parse warnings. Local diagnostic logs are
  ignored under `examples/out/public-c45820f0f9d74a21b40200af79d679b0.*.log`.
- This proves a short two-source data path on this machine, not long-term uptime,
  lossless history, profitability, all-venue coverage or release readiness.
- No push, remote CI, release tag or background daemon was started for delivery.

## 2026-09-12 — Prefunded paper scenarios and Bybit depth correctness

- Added fixed-pair paper ledger with explicit partial fill scenarios, inventory
  checks, remaining-depth reuse, fees and unmatched exposure. Wired HTTP, CLI
  and Python access; no inferred borrowing, maker fills or mark-to-market.
- Corrected Bybit snapshot/delta merge, zero-size deletion, restart reset and
  root source timestamps. Invalid state resets the builder; live research
  promotion remains withheld pending continuity validation.
- Aligned English/Chinese positioning and interface/architecture documentation
  with the research platform, while preserving the no-order boundary.
- Local Windows/MSVC: **288 Rust tests passed**; strict all-target/all-feature
  clippy, build, formatting and diff checks passed. **5 Python tests passed**.
- Authenticated localhost HTTP smoke passed, now including paired cash-cost
  reconciliation and unmatched-leg exposure with null closed-position PnL.
- No live-provider soak, remote CI, push, release tag or general portfolio claim.

## 2026-09-12 — Evidence-backed research API and recording foundation

- Added explicit instrument/relationship evidence, same-base cost curves,
  reference-only reason codes, bounded deterministic replay and local CLI.
- Added opt-in normalized recording with sequences, CRC and partial/sealed
  verification. Source drops remain visible; this is not raw exchange replay.
- Wired live spot cached-book evaluation with conservative snapshot promotion.
- Corrected historical/current feature mixing and timestamp alignment, net route
  ranking, sized depth calculations, hold timing and maker-fill assumptions.
- Added validation, custom-source instance/time identity, bounded HTTP with
  shared-origin cooldowns, zero-collector service lifetime and queue-close handling.
- Added local research config, API examples, standard-library Python client,
  explicit status inventory, usage, changelog and Windows/Linux CI definitions.
- Local Windows/MSVC evidence: `cargo test --locked --quiet` **280 passed**;
  `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
  `cargo build --locked` passed; formatting/diff checks passed.
- `scripts/Test-ResearchApi.ps1` passed against its own authenticated localhost
  process: cost/capacity, missing costs, replay, future rejection and crossed books.
- Python client contract tests: **4 passed**. CLI evaluated the shipped fixture.
- No push, remote CI result, live-provider soak, published version, or production
  readiness claim. Full roadmap remains incomplete; consult feature inventory.

## 2026-09-12 — Clock and freshness foundation

- Replaced timestamp-per-call increments with real observation time. Unique
  event ordering must use separate sequences; timestamps can repeat.
- Refresh quote stale status on reads, including predicates, without changing
  original receipt time or transport latency. Reject unknown/far-future times.
- Added deterministic fixed-clock regression tests (no sleeps).
- Adopted the generic research-only [platform roadmap](platform-roadmap.md).
- Repaired an incomplete local Rust toolchain (missing manifest). Rust 1.98.1
  now compiles the project on Windows/MSVC.
- Validation: the working-tree suite including these regressions passed
  `cargo +stable test --locked`: 270 passed, 0 failed. `cargo fmt` and
  `git diff --check` passed. No remote CI or live-provider certification claimed.

## 2026-09-14 — bull-call-spread quote persistence case

Added a Python-first options case for the public [Deribit bull-call-spread/options-flow
observation on X](https://x.com/laevitas1/status/1985373005644476891). The monitor
selects same-expiry call legs near configurable moneyness targets from
`/v1/options/chains`, prefers lower-call ask plus higher-call bid, labels mark-only
fallbacks, and emits debit, strike width, breakeven and capped paper payoff geometry.
The JSONL recorder and replay test whether the same expiry/strike identity stays
observable for a consecutive run. No order path, settlement, margin, hedge, cost,
fill or forward-PnL claim was added; all assumptions are documented bilingually in
`examples/crypto/options/README.md`.

## 2026-09-14 — macro context response lifecycle

Added a recorder/replay pair for the existing macro context monitor. The
recorder freezes DXY, VIX, US10Y, funding state and a BTC price snapshot from
MarketBridge; replay reports forward return, absolute return and downside
fractions by macro-volatility and funding-crowding bucket. It does not infer
causality or a direction, and explicitly retains the fact that macro quotes are
provider snapshots rather than synchronized historical index bars. Provenance:
[Wintermute's public macro/liquidity discussion on X](https://x.com/wintermute_t/status/1985631560021000352),
with VIX and dollar-index semantics cross-checked against the existing Cboe and
Federal Reserve references in the bilingual macro guide.

## 2026-09-14 — aggregate market-regime response lifecycle

Added a recorder/replay pair for `/v1/research/market-regime`. The recorder
freezes the Rust aggregate regime label beside a MarketBridge BTC quote; replay
reports forward return, absolute return and downside fractions for fragmented,
high-volatility, leveraged and normal states. It keeps the current-feature,
non-point-in-time limitation explicit and does not turn the label into a
strategy selector. Provenance: the unverified [XWIN trend and positioning
discussion on X](https://x.com/xwinfinance/status/2023155692916646257).

## 2026-09-14 — liquidation-burst response recorder and replay

Added a temporal companion to the bounded liquidation-burst replay. The new
Python recorder freezes each `/v1/history/liquidations` page beside a
synchronized MarketBridge quote in JSONL. The replay deduplicates repeated
observable event rows across snapshots, rebuilds rolling notional at each
capture, applies a configurable cooldown, and compares fixed-record BTC
responses after burst and ordinary windows. It keeps provider coverage,
side-label ambiguity, missing quotes and the no-order boundary explicit.

Provenance: [CryptoData's liquidation-threshold discussion on X](https://x.com/TheCryptoData/status/1948466627365769584)
is an unverified lead, while [Binance's liquidation-order stream documentation](https://developers.binance.com/en/docs/products/derivatives-trading-coin-futures/websocket-market-streams/Liquidation-Order-Streams)
defines the primary event-semantic boundary. This round adds no trading,
wallet or signing path.

## 2026-09-14 — options VRP response replay

Added the missing response layer for the existing options IV-minus-realized-
volatility recorder. The replay reuses its JSONL archive, classifies each
snapshot as implied-volatility premium, realized-volatility-above-implied or
aligned, and compares the next fixed-record BTC signed and absolute responses.
It retains selected-expiry roll, the different IV/RV horizons, candle coverage
and missing prices as evidence gaps; no option position, hedge, margin, fee or
execution path was introduced.

Provenance: the family guide keeps the unverified public [IV-minus-realized-volatility discussion on X](https://x.com/isellpremium/status/2072350364385349678)
and cross-checks the concept against [Deribit Insights' options data guide](https://insights.deribit.com/industry/genesis-volatility-options-data-guide/).

## 2026-09-14 — liquidity-stress response recorder and replay

Added a response lifecycle for the existing two-of-three liquidity-stress
context. The new recorder appends a synchronized perpetual quote alongside
book impact, spread and EWMA-volatility observations. The replay compares
fixed-record signed and absolute BTC movement after `liquidity_stress`,
`liquidity_watch` and `normal_liquidity` states, while excluding missing quotes
and retaining the snapshot-depth caveat. It is a risk-context response study,
not a directional, routing or execution strategy.

Provenance: the decomposition follows the unverified public [Pine Analytics /
FlyingTulip execution-risk discussion on X](https://x.com/PineAnalytics/status/1974474638093590994)
and the primary [Binance order-book API documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book).

## 2026-09-14 — universe opportunity candidate-response replay

Added a response lifecycle for the bounded universe opportunity scanner. The
new recorder freezes the point-in-time top-k candidate list beside perpetual
quotes and a BTC benchmark. The replay uses only that captured candidate list
to form an equal-weight paper basket, then compares its next fixed-record
return with BTC. Missing constituent prices remain a separate coverage state;
the result does not allocate capital, model turnover, or execute rebalancing.

Provenance: the candidate-discovery framing remains the unverified [RoboNet
multi-asset perpetuals discussion on X](https://x.com/RoboNetHQ/status/2024893544520143012),
cross-checked with [Binance's public funding-history documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info)
and MarketBridge's normalized universe endpoints.

## 2026-09-14 — spot/perp depth-gap response recorder and replay

Added the missing response layer for the existing same-venue spot/perpetual
depth-gap recorder. The new recorder freezes each target-size spot/perp book and
basis observation beside a perpetual MarketBridge quote. The replay compares
fixed-record signed and absolute BTC movement after perp-depth advantage,
spot-depth advantage and no-material-gap states. It keeps asynchronous books,
displayed-depth limitations, basis context and the no-execution boundary
explicit; no hedge route or arbitrage PnL is inferred.

Provenance: the public [spot/perp depth-gap discussion on X](https://x.com/ciaobelindazhou/status/2031929849850273955)
is an unverified lead, cross-checked against [Binance's official order-book documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book).

## 2026-09-14 — generic microstructure imbalance response replay

Added a response lifecycle for the generic top-level order-book imbalance
monitor. The recorder freezes bid/ask notional imbalance, funding-crowding
context and a perpetual quote; replay compares fixed-record signed and
absolute BTC movement after bid pressure, ask pressure, funding-conflict and
balanced-book states. Conflict labels remain separate, and missing quotes stay
outside the aligned sample. This is a descriptive microstructure study with
no fill, causality or execution model.

Provenance: the unverified [XWIN OI/order-flow discussion](https://x.com/xwinfinance/status/2023155692916646257)
is a research lead; book fields are cross-checked against [Binance's official order-book data documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Order-Book)
and funding fields against the [official funding-rate documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).

## 2026-09-14 — liquidation price-cluster response recorder and replay

Added a temporal response layer for the observed liquidation price-cluster
case. The recorder freezes bounded liquidation-history rows beside a
MarketBridge quote; replay deduplicates repeated rows, rebuilds rolling
price-band concentration, applies a cooldown, and compares fixed-record BTC
absolute movement after qualifying clusters versus ordinary windows. It keeps
the distinction from rolling liquidation bursts and does not reconstruct
latent heatmap levels, leverage, forecasts or execution.

Provenance: the public [CoinGlass liquidation-heatmap post on X](https://x.com/coinglass_com/status/1930154005491282291)
and [Glassnode's liquidation-heatmap research](https://research.glassnode.com/liquidation-heatmaps/)
motivate the falsifiable concentration hypothesis; event semantics remain
bounded by [Binance's liquidation-order stream documentation](https://developers.binance.com/en/docs/products/derivatives-trading-coin-futures/websocket-market-streams/Liquidation-Order-Streams).
## 2026-09-14 — Hyperliquid historical funding replay input

Added a read-only Hyperliquid `fundingHistory` adapter to
`/v1/history/candles?candle_type=funding_rate`. It normalizes public funding
timestamps and rates into the existing `KlineBar` shape, preserves provider coin
semantics, bounds an omitted window to the latest 30 days, and keeps the
adjacent-timestamp funding schedule visible to replay callers. This enables the
existing cross-venue funding-spread research examples to include Hyperliquid;
it does not estimate cash flow, invent a fixed interval, or add execution.

Provenance: the unverified [cross-venue funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
is a research lead, cross-checked against Hyperliquid's official
[`fundingHistory` API documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals#retrieve-historical-funding-rates).
## 2026-09-14 — Hyperliquid predicted-funding venue context

Added `/v1/market/predicted-funding` and the Python
`crypto_predicted_funding_monitor.py` example. The endpoint reads Hyperliquid's
public `predictedFundings` response, preserves each named provider venue and
next-funding timestamp, and never merges estimates into settled funding rows.
The example reports current same-coin dispersion and keeps single-venue or
missing-venue cases observe-only. This is a provider-context hypothesis and
does not estimate cash flow, execution, hedge PnL or orders.

Provenance: the unverified [cross-venue funding differential discussion on X](https://x.com/leondoteth/status/2012127303850213817)
and [cross-venue funding/arbitrage discussion](https://x.com/ranger_finance/status/2013284430174212491)
motivate the case; field semantics are cross-checked against Hyperliquid's
official [`predictedFundings` documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals#retrieve-predicted-funding-rates-for-different-venues).
## 2026-09-14 — Binance top-trader position-ratio scope

Extended `/v1/history/account-ratio` with
`scope=top_trader_position`, backed by Binance's public
`topLongShortPositionRatio` endpoint. The normalized row keeps a distinct
`source` and `semantics=top_trader_position_share`, so top-trader position
shares are not pooled with global account shares, top-trader account shares or
Bybit holder counts. The existing Python OI-response replay accepts the new
scope; the normalizer accepts both documented `longPosition/shortPosition`
fields and the USDⓈ-M `longAccount/shortAccount` shape without merging their
semantics. No position ledger, execution path or direction claim was added.

Provenance: Binance defines the endpoint as the long/short position share of
the top 20% of users by margin balance in its [official market-data documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Get-Funding-Info).
## 2026-09-14 — options skew monitor delta buckets

Extended `crypto_options_skew_monitor.py` with an explicit
`--bucket-mode delta` path. When normalized option greeks expose `delta`, the
monitor can compare ATM and approximately 25-delta put/call IV wings; missing
delta rows remain outside the comparable sample. The existing moneyness mode
remains the compatibility default, and no cross-venue delta convention is
assumed. This improves the falsifiable skew-response case without adding
option pricing, hedge construction or execution.

Provenance: the unverified [options brief on X](https://x.com/Gate_Launch/status/2063810805552845140)
motivates the 25-delta observation; normalized option fields are cross-checked
against [Binance's public options market-data documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-options/api/rest-api/market-data).

## 2026-09-14 — stablecoin supply liquidity context

Added `/v1/external/stablecoins` and the Python
`crypto_stablecoin_liquidity_monitor.py` example. The read-only endpoint
normalizes DefiLlama circulating supply, 24-hour/7-day/30-day changes and
chain distribution, while preserving provider snapshot limits. The monitor
classifies expansion, contraction and missing-change states as liquidity
context; it does not infer exchange inventory, bridge flow, price direction,
or execution.

Provenance: the unverified [stablecoin liquidity discussion on X](https://x.com/Cointelegraph/status/2029519994652942494)
and [stablecoin growth discussion](https://x.com/wintermute_t/status/1985631560021000352)
motivate the hypothesis. Data semantics are cross-checked against DefiLlama's
[stablecoin documentation](https://docs.llama.fi/).

## 2026-09-14 — stablecoin supply response replay

Extended the DefiLlama stablecoin context case with
`crypto_stablecoin_liquidity_response_recorder.py` and
`crypto_stablecoin_liquidity_response_replay.py`. The recorder freezes
expansion, contraction, and flat seven-day supply-change states beside a
synchronized MarketBridge BTC quote; replay compares fixed-record signed and
absolute responses with a minimum observation gate. Circulating supply remains
provider context rather than exchange flow, reserves, redemption pressure or
execution evidence.

Provenance remains the [stablecoin liquidity discussion on X](https://x.com/Cointelegraph/status/2029519994652942494)
and [stablecoin growth discussion](https://x.com/wintermute_t/status/1985631560021000352),
with fields cross-checked against DefiLlama's [stablecoin documentation](https://docs.llama.fi/).

## 2026-09-14 — Bitcoin mempool fee-pressure context

Added the keyless `/v1/onchain/mempool` endpoint. It combines mempool.space's
current mempool count/vsize/aggregate fee, recommended sat/vB rates, and tip
height into a bounded `bitcoin_mempool_context` response. The Python
`crypto_onchain_mempool_pressure_monitor.py` classifies high, low, ordinary and
missing-metric states; its recorder/replay pair compares fixed-record BTC
responses without treating fee pressure as a directional signal.

The endpoint is deliberately on-demand and separate from the optional large
transfer collectors. It does not expose wallet actions, transaction broadcast,
confirmation guarantees or execution. The research lead is [Alex Thorn's public
X observation about an empty Bitcoin mempool](https://x.com/intangiblecoins/status/2043001350628184497);
field semantics and limitations are cross-checked against the [official
mempool.space REST API](https://mempool.space/docs/api/rest) and [FAQ](https://mempool.space/docs/faq).

## 2026-09-14 — Bitcoin mining difficulty/hashrate context

Added the keyless `/v1/onchain/mining` endpoint. It normalizes mempool.space's
public difficulty-adjustment snapshot and trailing one-week hashrate sample into
`bitcoin_mining_context`, including difficulty change, retarget progress,
current hashrate, and the observable seven-day hashrate change. The Python
monitor/recorder/replay classifies miner-stress, tailwind and ordinary states,
then compares fixed-record BTC responses without claiming miner identity,
profitability, capitulation or forced selling.

Provenance: the unverified [CryptoDiffer difficulty-drop discussion on X](https://x.com/CryptoDiffer/status/2021059510106980651)
is treated only as a research lead. Field semantics are cross-checked against
the [official mempool.space REST API](https://mempool.space/docs/api/rest).
