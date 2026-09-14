# Crypto microstructure, squeeze and liquidation

> **Question:** do observable flow, depth, OI, liquidation, or volatility states
> differ in later price response without pretending to reveal trader intent?

## Case map

| Evidence | Entrypoints |
|---|---|
| Confluence monitors | `short_squeeze_monitor.py`, `exhaustion_short_monitor.py`, `liquidation_reversal_monitor.py`, `crypto_microstructure_monitor.py` |
| Flow and depth | `crypto_flow_book_confirmation.py`, `crypto_footprint_imbalance_*`, `crypto_spot_perp_depth_gap_*`, `crypto_liquidity_stress_*` |
| Two-sided walls | `crypto_liquidity_sandwich_monitor.py`, `crypto_liquidity_sandwich_response_recorder.py`, `crypto_liquidity_sandwich_response_replay.py` |
| Liquidation studies | `crypto_liquidation_burst_*`, `crypto_liquidation_price_cluster_*`, `crypto_liquidation_intensity_response_replay.py`, `liquidation_reversal_replay.py` |
| Event/technical replay | `crypto_cvd_divergence_replay.py`, `crypto_obv_divergence_response_replay.py`, `crypto_keltner_channel_response_replay.py`, `crypto_donchian_channel_response_replay.py`, `crypto_trade_imbalance_bar_replay.py`, `crypto_vpin_response_replay.py`, `crypto_*vwap*`, `crypto_*breakout*`, `crypto_*fair_value_gap*`, `crypto_session_*`, `crypto_weekly_rsi_cross_response_replay.py`, `crypto_weekday_hour_effect_replay.py` |
| Derivatives crowding | `crypto_taker_oi_response_replay.py`, `crypto_oi_price_divergence_response_replay.py`, `crypto_account_ratio_oi_response_replay.py`, `crypto_derivatives_*`, `crypto_adl_risk_*` |

The recorder/replay pairs freeze a state beside a quote and measure a later
fixed-record signed or absolute return. The ADL pair treats Binance's rating as
provider context—not proof that ADL occurred or a private account was at risk.
The live liquidation store now retains a bounded recent event window per
venue/symbol instead of overwriting the previous event. Pass
`--source market` to `crypto_liquidation_burst_response_recorder.py` to archive
Binance, Bybit, BitMEX, Gate, and other enabled live feeds; use
`--source history` for the OKX/CoinEx bounded-history path. Retention is not a
complete historical ledger and feed gaps remain visible.
The liquidity-sandwich pair tests the narrower public-X claim that symmetric
near-touch bid and ask depth with a tight spread is followed by a different
absolute BTC move than ordinary snapshots. It does not call the displayed
levels persistent walls or infer a range-trading opportunity.

`crypto_liquidity_sweep_response_replay.py` tests the measurable OHLCV subset
of a public liquidity-sweep/reclaim discussion. It finds a current candle that
trades beyond the prior lookback high or low, closes back through that level,
and has a directional body/range. It then reports direction-aligned forward
returns. This is not proof of resting stop liquidity, a true liquidity pool,
CISD, displacement intent, or an executable setup.

```bash
python3 examples/crypto/microstructure/crypto_liquidity_sweep_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 15m \
  --days 15 --lookback-bars 20 --sweep-buffer-bps 0 \
  --min-body-fraction 0.50 --min-range-bps 5 \
  --horizon-bars 8 --paper-cost-bps 10 --min-observations 5
```

The research lead is [KM Trading's public X setup breakdown](https://x.com/KMTrading_SMC/status/2032428981040103847).
MarketBridge uses the official [Binance kline/candlestick field semantics](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data)
and keeps the X terminology narrower than the post: only prior-range breach,
reclaim and candle geometry are tested.

`crypto_atr_regime_response_replay.py` is a separate volatility-context study.
It computes a simple-average true range, classifies the current value against a
trailing as-of distribution, and compares compressed/ordinary/expanded states
with later signed and absolute BTC returns. It does not forecast direction,
size positions, place stops, or execute trades. The research lead is the
[public regime/ATR discussion on X](https://x.com/viviennaBTC/status/2037854988442235187),
and the calculation is cross-checked against [Binance Academy's ATR explanation](https://www.binance.com/en/square/post/510812).

```bash
python3 examples/crypto/microstructure/crypto_atr_regime_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --atr-period 14 --regime-lookback 96 \
  --low-quantile 0.20 --high-quantile 0.80 \
  --horizon-bars 8 --min-observations 5
```

`crypto_breakout_retest_response_replay.py` tests the complementary
continuation hypothesis: a close beyond a prior range followed by a bounded
touch of the broken level and a close back on the breakout side. The forward
window starts at the retest close, so the retest is observable before the
response is measured. It does not infer true support/resistance, resting
orders, volume confirmation, or fills. The research lead is
[Rekt Capital's public BTC breakout/retest discussion](https://x.com/rektcapital/status/1850982324621676715),
cross-checked with [Binance Academy's crypto breakout guidance](https://www.binance.com/en/academy/articles/a-beginners-guide-to-swing-trading-cryptocurrency).

```bash
python3 examples/crypto/microstructure/crypto_breakout_retest_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --lookback-bars 24 --breakout-buffer-bps 2 \
  --retest-window 8 --retest-tolerance-bps 15 --horizon-bars 8 \
  --paper-cost-bps 10 --min-observations 5
```

`crypto_ichimoku_cloud_response_replay.py` implements an as-of Ichimoku
response table. It evaluates the price/cloud position, Tenkan/Kijun relation,
cloud color and Chikou comparison, while reading the visible Senkou spans from
their displaced historical calculation. This prevents the common mistake of
using a future-projected cloud as if it were known today. The states are
descriptive; there is no forecast, allocation, stop or execution path. The
research lead is an [unverified public Ichimoku/cloud discussion on X](https://x.com/Invst_Informant/status/2014788740992929906),
and formulas are cross-checked against [Binance Academy's Ichimoku guide](https://www.binance.com/en/academy/articles/ichimoku-clouds-explained).

```bash
python3 examples/crypto/microstructure/crypto_ichimoku_cloud_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --conversion-period 9 --base-period 26 \
  --span-b-period 52 --displacement 26 --horizon-bars 6 \
  --min-observations 5
```

`crypto_rsi_bollinger_extreme_response_replay.py` is the combined-extreme
case. It separates RSI-only extremes, Bollinger-only band breaches, joint
overbought/oversold confluence, and ordinary candles before measuring later
responses. It does not assume that an extreme must reverse; persistent trends,
indicator conventions and parameter sensitivity remain visible. The research
lead is [a public BTC RSI-plus-upper-Bollinger discussion on X](https://x.com/MichaelMOTTCM/status/1944846581611814956),
with definitions cross-checked against [Binance's RSI glossary](https://www.binance.com/en/academy/glossary/relative-strength-index)
and [Bollinger Bands explanation](https://www.binance.com/en/square/post/42841).

```bash
python3 examples/crypto/microstructure/crypto_rsi_bollinger_extreme_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --band-period 20 --deviations 2 \
  --overbought 70 --oversold 30 --horizon-bars 12 \
  --min-observations 5
```

`crypto_fibonacci_retracement_response_replay.py` tests a narrower, point-in-time
retracement hypothesis. It selects a trailing-window high and low, determines
their chronological direction, and groups the current close near the 38.2%,
50%, or 61.8% ratios (plus control ranges). The anchors end before the current
candle, so the study does not use future swing information. It reports later
signed, direction-aligned, and absolute returns; it does not claim that a level
is support/resistance or create an entry, stop, or order rule. The research lead
is [a public WIF Fibonacci discussion on X](https://x.com/CryptoJournaal/status/2026693734063075685),
with definitions cross-checked against [Binance Academy's Fibonacci guide](https://www.binance.com/en/academy/articles/a-guide-to-mastering-fibonacci-retracement)
and [glossary](https://www.binance.com/en/academy/glossary/fibonacci-retracement).
Windows with ambiguous same-candle extrema remain visible as `missing_swing`
instead of being assigned an arbitrary direction.

```bash
python3 examples/crypto/microstructure/crypto_fibonacci_retracement_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 90 --level-tolerance 0.03 \
  --horizon-bars 6 --min-observations 5
```

`crypto_fair_value_gap_response_replay.py` tests a separate three-candle
imbalance proxy. Candle one and candle three must leave a wick non-overlap,
while the middle candle must meet an explicit body-fraction filter. The replay
then records untouched, touched, and wick-filled zones and compares bullish,
bearish, and ordinary-bar responses. It does not claim an untraded volume void,
institutional intent, support/resistance, or a fill. The research lead is
[a public FVG discussion on X](https://x.com/Bradgohtrades/status/2058684241958031814),
cross-checked with [Binance Academy's candlestick guidance](https://www.binance.com/en/square/post/492082)
and the [official kline field documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_fair_value_gap_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 90 --min-gap-bps 5 --min-middle-body-fraction 0.50 \
  --horizon-bars 8 --min-observations 5
```

`crypto_obv_divergence_response_replay.py` is deliberately separate from CVD:
it applies the classic close-signed OBV rule to candle volume, normalizes the
change by total volume in an as-of lookback window, and compares bullish/bearish
divergence with price/OBV agreement and mixed controls. It does not observe
aggressive trade side, ownership, accumulation, or whale intent. Missing volume
is retained as `missing_volume`, not replaced with zero. Definitions are
cross-checked against [Binance's OBV explanation](https://www.binance.com/en/square/post/1218711)
and [Fidelity's OBV formula and limitations](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/OBV).

```bash
python3 examples/crypto/microstructure/crypto_obv_divergence_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 4h \
  --days 730 --lookback-bars 20 --price-threshold-bps 20 \
  --obv-threshold 0.10 --horizon-bars 6 --min-observations 5
```

`crypto_keltner_channel_response_replay.py` uses an explicit EMA centerline
and trailing simple true-range ATR width. It separates first crosses above/below
the channel from persistent outside states and inside-channel controls, then
reports aligned, absolute and path responses. This is not a platform-specific
indicator guarantee, breakout rule, reversal rule, stop, or execution model.
Definitions are cross-checked against [Binance Academy's Keltner comparison](https://academy.binance.com/lt/articles/bollinger-bands-explained)
and [Binance's ATR/Keltner tutorial](https://www.binance.com/pt/square/post/22339649998217).

```bash
python3 examples/crypto/microstructure/crypto_keltner_channel_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --ema-period 20 --atr-period 10 --atr-multiplier 2 \
  --horizon-bars 8 --min-observations 5
```

`crypto_donchian_channel_response_replay.py` uses only the prior lookback
high and low, excluding the current candle, to separate first upper/lower
breaks, persistent outside states, and inside-channel controls. It reports
later aligned, absolute and path responses; it does not call rolling extrema
true support/resistance or create a trend, stop, or execution rule. Field
semantics are cross-checked against [Binance's ATR/Keltner/Donchian tutorial](https://www.binance.com/pt/square/post/22339649998217)
and [official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_donchian_channel_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --lookback-bars 20 --horizon-bars 8 --min-observations 5
```

`crypto_supertrend_response_replay.py` is a transparent ATR-band response study.
It computes a trailing simple true-range ATR, builds midpoint ± multiplier bands,
and recursively clamps the active band using only the current and prior candles.
A close crossing the prior active direction is labelled `bullish_flip` or
`bearish_flip`; remaining valid bars are `bullish_trend` or `bearish_trend`
controls. The replay compares fixed-horizon aligned, absolute and path responses.
The commonly used `atr-period=10` and `multiplier=3` are sensitivity starting
points, not a promise of TradingView/exchange smoothing parity. States are
descriptive research labels, never entry, stop, forecast or order instructions.
The research lead is [Binance Square's Supertrend parameter/readout note](https://www.binance.com/en/square/post/24192142459218),
and candle fields are bounded by [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_supertrend_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --atr-period 10 --multiplier 3 --horizon-bars 8 \
  --min-observations 5
```

`crypto_adx_dmi_response_replay.py` turns the public ADX/DMI story into a
falsifiable strength study. It applies an explicit Wilder-style recursive
smoothing convention to true range, +DM, -DM, +DI, -DI, DX and ADX, then groups
each as-of bar into `strong_bullish`, `strong_bearish`, `strong_mixed`, weak
directional states, or `range_or_mixed`. A +DI/-DI direction change is retained
as a separate `bullish_di_cross` or `bearish_di_cross` event; strong states are
compared with weak-direction/range controls over a fixed horizon. ADX describes historical directional
strength; it does not prove continuation, intent, entry, stop or execution.
Period 14 and threshold 25 are sensitivity starting points. The research lead
is [Binance Square's ADX/DMI note](https://www.binance.com/en/square/post/15751696197586);
warmup and smoothing boundaries are cross-checked against [TradingView's DMI calculation](https://www.tradingview.com/support/solutions/43000502250-directional-movement-dmi/),
and candle fields use [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_adx_dmi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --strength-threshold 25 --horizon-bars 8 \
  --min-observations 5
```

`crypto_aroon_response_replay.py` tests whether the recency of the latest
lookback high or low is followed by a different response. Using an inclusive
as-of OHLC window, it computes Aroon Up/Down and their oscillator, separates
`bullish_recent_extreme`, `bearish_recent_extreme`, `consolidation` and
`balanced`, and keeps Aroon direction crosses as separate events. Strong
recent-extreme states are compared with consolidation controls using fixed-
horizon signed, direction-aligned, absolute and path responses. “Trend age” is
not a price forecast, entry, stop or execution rule; the window, 70/50
thresholds and most-recent tie rule are explicit parameters. Formula boundaries
are cross-checked against [TradingView's Aroon documentation](https://www.tradingview.com/support/solutions/43000501801-aroon-indicator/),
crypto terminology against [CoinMarketCap's Aroon glossary](https://coinmarketcap.com/academy/glossary/aroon-indicator/),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_aroon_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --trend-threshold 70 \
  --consolidation-threshold 50 --horizon-bars 8 --min-observations 5
```

`crypto_mfi_response_replay.py` turns the “price plus volume money flow” story
into a falsifiable extreme study. It multiplies typical price by candle volume,
accumulates positive and negative flow according to the typical-price change,
and computes a bounded 0–100 MFI. Bars are grouped as `overbought`, `oversold`
or `neutral`; `oversold_reclaim` and `overbought_rejection` are retained as
separate events, and extreme states are compared with neutral controls using
fixed-horizon signed, aligned, absolute and path responses. Volume is candle
volume—not aggressive flow, exchange net flow or position ownership. The 80/20
thresholds, period and horizon are sensitivity parameters, not reversal, entry
or execution rules. Formula details are cross-checked against [TradingView's MFI calculation](https://www.tradingview.com/support/solutions/43000502348-money-flow-mfi/);
the public crypto research lead is [a Binance Square money-flow discussion](https://www.binance.com/en/square/post/21507916375097),
and candle fields use [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_mfi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --overbought 80 --oversold 20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_cmf_response_replay.py` is deliberately separate from MFI and OBV. It
computes each candle's `(close-low)/(high-low)` money-flow multiplier, multiplies
it by volume, and divides rolling money-flow volume by rolling volume. Bars are
classified as `positive_pressure`, `negative_pressure` or `neutral`, while
`positive_zero_cross` and `negative_zero_cross` remain separate events. Pressure
states are compared with neutral controls over a fixed horizon. A candle with
`high == low` is not filled with an artificial pressure value; any window that
needs it remains missing. This is a close-location-weighted OHLCV proxy, not
aggressive flow, exchange net flow, position ownership or execution. Period,
thresholds and horizon are sensitivity parameters. Formula details are
cross-checked against [TradingView's Chaikin Money Flow documentation](https://www.tradingview.com/support/solutions/43000501974-chaikin-money-flow-cmf/),
the public crypto research lead is [a Binance Square money-flow discussion](https://www.binance.com/en/square/post/21507916375097),
and candle fields use [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_cmf_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 21 --positive-threshold 0.05 \
  --negative-threshold -0.05 --horizon-bars 8 --min-observations 5
```

`crypto_parabolic_sar_response_replay.py` implements an explicit Wilder-style
Extreme Point (EP) and Acceleration Factor (AF) recursion. It labels each
point-in-time bar as `bullish_trend` or `bearish_trend`, retains
`bullish_flip`/`bearish_flip` events, and compares flips with persistent-trend
controls over a fixed horizon. Although SAR stands for Stop and Reverse, this
case only studies states: it never submits a stop, reversal, entry or order.
Initialization, the two-prior-high/low clamp, `start-af`, `step` and `max-af`
are explicit parameters. Formula and warmup boundaries are cross-checked
against [TradingView's Parabolic SAR documentation](https://www.tradingview.com/support/solutions/43000502597-parabolic-sar-sar/),
crypto usage context against [Binance Academy's SAR guide](https://www.binance.com/en/square/post/43032),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_parabolic_sar_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --start-af 0.02 --step 0.02 --max-af 0.20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_stochastic_response_replay.py` computes raw `%K` from the current close
inside an inclusive high/low window, then applies contiguous trailing SMAs for
smoothed `%K` and `%D`. Bars are classified as `overbought`, `oversold` or
`neutral`; `bullish_kd_cross` and `bearish_kd_cross` remain separate events,
and extremes are compared with neutral controls over a fixed horizon. A
zero-range or incomplete smoothing window stays missing rather than crossing a
gap. The 80/20 thresholds, 14/3/3 periods and horizon are sensitivity
parameters, not reversal, entry, stop or execution rules. Formula boundaries
are cross-checked against [TradingView's Stochastic documentation](https://www.tradingview.com/support/solutions/43000502332-stochastic-stoch/),
crypto usage context against [Binance's overbought/oversold note](https://www.binance.com/en/square/post/684815),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_stochastic_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --k-period 14 --smooth-k 3 --smooth-d 3 \
  --overbought 80 --oversold 20 --horizon-bars 8 --min-observations 5
```

`crypto_stochrsi_response_replay.py` first computes RSI with an explicit Wilder
smoothing convention, then places the current RSI inside its own trailing
RSI high/low range and applies contiguous SMAs for StochRSI `%K/%D`. Bars are
classified as `overbought`, `oversold` or `neutral`; `bullish_kd_cross` and
`bearish_kd_cross` remain separate events and extremes are compared with
neutral controls over a fixed horizon. It is an indicator of an indicator and
can be noisier than RSI: constant-RSI and incomplete smoothing windows stay
missing. The 80/20, 14/14/3/3 and horizon values are sensitivity parameters,
not reversal, entry, stop or execution rules. Formula boundaries are
cross-checked against [TradingView's StochRSI documentation](https://www.tradingview.com/support/solutions/43000502333-stochastic-rsi-stoch-rsi/),
crypto context against [Binance Academy's StochRSI guide](https://www.binance.com/en/square/post/511150),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_stochrsi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --rsi-period 14 --stoch-period 14 \
  --smooth-k 3 --smooth-d 3 --overbought 80 --oversold 20 \
  --horizon-bars 8 --min-observations 5
```

`crypto_vortex_response_replay.py` computes `VM+ = |high - prior low|` and
`VM- = |low - prior high|`, then normalizes each sum by the same trailing
true-range sum to produce `VI+` and `VI-`. Bars are grouped as
`bullish_pressure`, `bearish_pressure` or `balanced`; `bullish_vi_cross` and
`bearish_vi_cross` remain separate events, and pressure is compared with
balanced controls over a fixed horizon. `minimum-spread`, period and horizon
are sensitivity parameters; zero-range/missing windows stay unavailable and a
cross is not a reversal, entry, stop or execution rule. Formula boundaries are
cross-checked against [TradingView's Vortex Indicator documentation](https://www.tradingview.com/support/solutions/43000591352-vortex-indicator/),
and candle fields use [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_vortex_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --period 14 --minimum-spread 0.05 \
  --horizon-bars 8 --min-observations 5
```

`crypto_pivot_response_replay.py` fixes the pivot timeframe to a UTC day. It
uses the previous UTC day's high/low/close to compute traditional `P`, `R1/S1`
and `R2/S2`, then labels each next-day candle as `r1_rejection`, `s1_reclaim`,
`above_r1`, `below_s1`, `inside_pivot_range` or `near_pivot`. Events are
compared with inside-range controls using fixed-horizon signed, aligned,
absolute and path responses. These are OHLC level proxies, not validated
support/resistance; UTC boundaries, near tolerance and horizon are explicit
parameters and never create an entry, stop or order. Formula boundaries are
cross-checked against [TradingView's Pivot Points Standard documentation](https://www.tradingview.com/support/solutions/43000521824-pivot-points-standard/),
crypto intraday context against [Binance's pivot/support-resistance note](https://www.binance.com/en/square/post/375172),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_pivot_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --tolerance-bps 10 --horizon-bars 8 --min-observations 5
```

`crypto_heikin_ashi_response_replay.py` recursively builds synthetic
Heikin-Ashi OHLC, separates `bullish_no_lower_wick`, `bearish_no_upper_wick`,
mixed-direction and `doji` states, and retains bullish/bearish flips. The
critical boundary is that synthetic candles are labels only: every forward
response uses the MarketBridge source-candle close, never a synthetic price.
Wick tolerance, doji threshold and horizon are explicit sensitivity parameters;
no stop, entry or execution rule is produced. Formula and synthetic-price
limits are cross-checked against [TradingView's Heikin-Ashi documentation](https://www.tradingview.com/support/solutions/43000619436-understanding-heikin-ashi-charts/),
crypto context against [Binance's Heikin-Ashi guide](https://www.binance.com/en/square/post/474846),
and candle fields against [Binance's official kline documentation](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_heikin_ashi_response_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 1h \
  --days 180 --wick-tolerance-bps 1 --doji-body-bps 5 \
  --horizon-bars 8 --min-observations 5
```

`crypto_liquidation_intensity_response_replay.py` is the normalized companion
to the absolute liquidation-burst and price-cluster cases. It divides observed
liquidation notional by typical-price times base-volume turnover over the same
trailing candle window, then compares high-intensity and ordinary windows by
later absolute movement. The event venue and price venue remain explicit;
side labels are metadata, and bounded history is not a complete cascade ledger.
The ratio-based research lead is [a liquidation-aware backtesting framework](https://candlefeed.ai/blog/liquidation-aware-backtesting/),
with event semantics cross-checked against [Binance's liquidation stream documentation](https://developers.binance.com/en/docs/derivatives/coin-margined-futures/websocket-market-streams/Liquidation-Order-Streams)
and [official kline fields](https://developers.binance.com/docs/derivatives/coin-margined-futures/market-data/rest-api/Kline-Candlestick-Data).

```bash
python3 examples/crypto/microstructure/crypto_liquidation_intensity_response_replay.py \
  --liquidation-exchange okx --price-exchange okx --symbol BTCUSDT \
  --interval 5m --window-bars 12 --horizon-bars 12 \
  --min-intensity-ratio 0.01 --min-observations 3
```

`crypto_weekly_rsi_cross_response_replay.py` tests a separate close-only
hypothesis on `1w` candles: after weekly RSI(14) crosses its own 14-week simple
average, does the next fixed number of weekly closes show a different signed
return or minimum path return? RSI is implemented explicitly as a simple
average-gain/loss oscillator; the output does not inherit any platform-specific
indicator convention or turn an X post into a forecast.

```bash
python3 examples/crypto/microstructure/crypto_weekly_rsi_cross_response_replay.py \
  --exchange binance --symbol BTCUSDT --interval 1w --days 3650 \
  --rsi-period 14 --rsi-sma-period 14 --horizon-weeks 4 \
  --min-observations 3
```

## Quickstart

```bash
python3 examples/crypto/microstructure/crypto_adl_risk_monitor.py \
  --symbol BTCUSDT --exchange binance
python3 examples/crypto/microstructure/crypto_adl_risk_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 30 --interval-secs 60 \
  --output work/crypto-adl-risk-response.jsonl
python3 examples/crypto/microstructure/crypto_adl_risk_response_replay.py \
  --input work/crypto-adl-risk-response.jsonl --horizon-records 3 \
  --min-observations 5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_monitor.py \
  --symbol BTCUSDT --exchange binance --depth-band-bps 10 \
  --min-side-depth-notional 100000 --min-symmetry-ratio 0.5
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_recorder.py \
  --symbol BTCUSDT --exchange binance --iterations 60 --interval-secs 30 \
  --output work/crypto-liquidity-sandwich-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidity_sandwich_response_replay.py \
  --input work/crypto-liquidity-sandwich-response.jsonl \
  --horizon-records 3 --min-observations 5
```

The full command set remains in [`README.md`](README.md). First polls may have
no OI baseline; venue liquidation side, trade side and book semantics are
provider-specific and must stay in the output.

The OI/price quadrant replay aligns each price candle with the latest
non-future OI row, keeps OI age and provider units visible, classifies four
observable price/OI states, and measures later signed and absolute returns.
The labels do not prove short covering, new shorts, long liquidation or trader
intent.

```bash
python3 examples/crypto/microstructure/crypto_oi_price_divergence_response_replay.py \
  --symbol BTCUSDT --price-exchange binance --oi-exchange okx \
  --price-interval 5m --oi-interval 5m --days 2 \
  --lookback-bars 3 --horizon-bars 3 --min-observations 5
```

The decomposition is motivated by [TheCryptoData's public OI and liquidation
discussion on X](https://x.com/TheCryptoData/status/1948466627365769584) and
uses the public [OKX contract OI documentation](https://www.okx.com/docs-v5/en/#rest-api-trading-data-get-contracts-open-interest-and-volume)
alongside the existing Binance history semantics. These are research inputs,
not evidence of a profitable divergence trade.

```bash
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_recorder.py \
  --source market --exchange binance --price-exchange binance \
  --symbol BTCUSDT --iterations 120 --interval-secs 30 \
  --output work/crypto-live-liquidation-burst-response.jsonl
python3 examples/crypto/microstructure/crypto_liquidation_burst_response_replay.py \
  --input work/crypto-live-liquidation-burst-response.jsonl \
  --window-hours 1 --horizon-records 12 --threshold-notional 1000000 \
  --cooldown-records 12 --min-observations 3
```

## Evidence rules

- A confluence score is a screening observation, not an entry or exit signal.
- Missing OI, flow, liquidation or quote data is `observe_only`, never zero.
- A rolling buffer, candle approximation, or aggregate long/short ratio does
  not reveal ownership, intent, dealer sign, latent liquidation levels, or causality.
- Costs, funding, borrow, latency, slippage, queue position and fills are not
  silently inferred by these examples.

Provenance links and provider coverage notes are maintained in the source
catalog and include Binance's [ADL Risk API](https://developers.binance.com/docs/derivatives/usds-margined-futures/market-data/rest-api/ADL-Risk)
and public research leads such as this [liquidation discussion on X](https://x.com/angustias87/status/2039147109228925373).

## Boundary

This family never places orders, liquidates positions, signs wallets, or
interprets public aggregates as a user's private account state.
