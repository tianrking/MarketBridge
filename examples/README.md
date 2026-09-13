# MarketBridge strategy demo library

These examples are research observers. They call the running MarketBridge HTTP
API, print evidence and a bounded hypothesis score, and never place orders.
Each demo must state its data assumptions and keep missing data visible.

Strategy code is Python-first. Rust remains the framework/runtime layer for
connectors, normalization, caches, history, replay primitives and API serving.
The older Rust strategy examples remain compatibility references; new strategy
experiments should start from `python_strategy_runner.py`.

## Current cases

| Demo | Strategy hypothesis | MarketBridge inputs | Status |
|---|---|---|---|
| `short_squeeze_monitor` | Negative funding + rising OI + spot/perp flow divergence can identify a squeeze candidate | funding, OI, order flow, liquidations, external liquidation signal | Research observer |
| `exhaustion_short_monitor` | Positive funding + failed highs + falling OI + weak bids can identify long exhaustion | funding, OI, klines, order flow, L2, optional on-chain transfers | Research observer |
| `basis_carry_monitor` | Positive spot/perp basis + positive funding can justify a delta-neutral carry investigation | basis, funding, observed funding interval when available | Research observer; withholds annualization when interval is unknown |
| `liquidation_reversal_monitor` | Sell-side liquidation + falling OI + positive CVD and price recovery can identify a flush-reversal candidate | liquidations, OI, order flow, klines | Research observer; liquidation side semantics must be venue-validated |
| `liquidation_reversal_replay.py` | Measure forward price recovery after bounded OKX/CoinEx sell-side liquidation events, optionally joined with public OI | `/v1/history/liquidations`, `/v1/history/candles`, `/v1/history/open-interest` | Partial replay; CoinEx uses `--price-exchange okx|binance`; historical CVD and execution costs remain explicit gaps |
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
| `crypto_session_filter.py` | Test a short session-window hypothesis with VWAP, EMA(9/21), MACD and volume confirmation | `/v1/market/klines` | Research filter; no universal timing edge or fill model |
| `crypto_volatility_breakout_replay.py` | A range break after compressed realized volatility may continue when candle volume and optional taker flow confirm | `/v1/history/candles`, optional `/v1/history/trades` | Bounded close-to-close replay; no execution, funding or fee model |
| `crypto_options_skew_monitor.py` | Put-wing IV minus call-wing IV and near/far ATM IV term structure expose options hedging demand and volatility regime | `/v1/options/chains` | Snapshot observer using transparent moneyness buckets; no delta-hedge or execution model |
| `crypto_options_skew_recorder.py` | Freeze repeated options skew snapshots so persistence can be tested rather than inferred from one quote | `/v1/options/chains` | Append-only JSONL observation archive; no private ledger or order data |
| `crypto_options_skew_replay.py` | Measure skew persistence and term-state runs from recorded snapshots | explicit JSONL from recorder | Descriptive persistence replay; no option PnL or hedge simulation |
| `crypto_options_vrp_monitor.py` | Compare selected-expiry ATM mark IV with annualized perp realized volatility | `/v1/options/chains`, `/v1/history/candles` | Snapshot IV-minus-RV observer; maturity, hedge and cost basis stay explicit |
| `funding_convergence_monitor.py` | Compare explicit hourly funding rates across venues and flag a gross differential for investigation | `/v1/market/perpetual-funding` | Withholds annualization when provider interval is unknown; no hedge execution |
| `funding_convergence_replay.py` | Align historical funding observations and measure differential persistence across venues | `/v1/market/perpetual-funding`, `/v1/history/candles` | Uses point-in-time adjacent timestamp intervals; no fill, cost or hedge simulation |
| `crypto_funding_oi_replay.py` | Extreme funding plus rising OI may identify crowded longs/shorts whose next price window moves against the crowd | `/v1/history/candles`, `/v1/history/open-interest` | Venue and schedule gaps remain explicit; forward return is not a hedge PnL |
| `crypto_microstructure_monitor.py` | Top-of-book bid/ask depth imbalance can identify short-term pressure, while extreme funding is a crowding warning | `/v1/market/order-books`, `/v1/market/perpetual-funding` | Snapshot observer; missing books and funding conflicts stay explicit |
| `crypto_flow_book_confirmation.py` | Same-direction taker-flow delta/CVD confirms an L2 pressure candidate; opposite flow rejects it | `/v1/market/order-books`, `/v1/market/order-flow`, `/v1/market/perpetual-funding` | Point-in-time confirmation observer; no execution, fill or cost model |
| `python_strategy_runner.py` | Python-first versions of squeeze, exhaustion, basis, liquidation, volatility breakout, options skew and options VRP observers | normalized MarketBridge endpoints | Primary strategy entry point; read-only JSON output |
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
cargo run --example short_squeeze_monitor -- \
  --symbol BTCUSDT --exchange binance --iterations 3
cargo run --example exhaustion_short_monitor -- \
  --symbol BTCUSDT --exchange binance --iterations 3
cargo run --example basis_carry_monitor -- \
  --symbol BTCUSDT --exchange binance --iterations 3
cargo run --example liquidation_reversal_monitor -- \
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
python3 examples/crypto_session_filter.py \
  --exchange binance --market perp --symbol BTCUSDT \
  --interval 1m --limit 60 --timezone America/New_York
python3 examples/crypto_volatility_breakout_replay.py \
  --exchange binance --symbol BTCUSDT --market perp --interval 5m \
  --days 3 --range-bars 12 --compression-window 12 \
  --baseline-window 48 --flow-exchange binance --flow-pages 12
python3 examples/crypto_options_skew_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --min-skew-iv 3 --min-term-slope-iv 3
python3 examples/crypto_options_skew_recorder.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --iterations 20 --interval-secs 30 \
  --output work/crypto-options-skew.jsonl
python3 examples/crypto_options_skew_replay.py \
  --input work/crypto-options-skew.jsonl --min-skew-iv 3 --min-run 3
python3 examples/crypto_options_vrp_monitor.py \
  --currency BTC --venue deribit --expiry-days 30 \
  --price-exchange binance --symbol BTCUSDT --interval 1h --rv-bars 168 \
  --vrp-threshold 5
python3 examples/funding_convergence_monitor.py \
  --symbol BTCUSDT --exchanges binance,okx,bybit \
  --iterations 3 --interval-secs 30
python3 examples/funding_convergence_replay.py \
  --symbol BTCUSDT --exchanges binance,bybit --days 7 --limit 200
python3 examples/crypto_funding_oi_replay.py \
  --symbol BTCUSDT --funding-exchange binance \
  --oi-exchange binance --price-exchange binance \
  --days 7 --min-funding-pct 0.01 --min-oi-change-pct 0.10
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
- [Cross-venue funding differential narrative (unverified public claim)](https://x.com/leondoteth/status/2012127303850213817)
- [Funding/OI/liquidation context snapshot (unverified public claim)](https://x.com/ImCryptOpus/status/1949195275903410571)
- [L2 imbalance plus funding-extreme perp logic (unverified public claim)](https://x.com/instaclaws/status/2038363051213181035)
- [Realized-volatility compression context (unverified public claim)](https://x.com/glassnode/status/1955218957490594099)
- [Breakout confirmation / hold-above-level context (unverified public claim)](https://x.com/rektcapital/status/1893996786173259958)
- [BTC/ETH ATM IV and 25D skew options brief (unverified public claim)](https://x.com/Gate_Launch/status/2063810805552845140)
- [Deribit bull-call-spread / options-flow observation (unverified public claim)](https://x.com/laevitas1/status/1985373005644476891)
- [IV minus realized-volatility dashboard / VRP context (unverified public claim)](https://x.com/isellpremium/status/2072350364385349678)
- [Compression-to-expansion / low-volume-node context (unverified public claim)](https://x.com/Stoiiic/status/1796078958674628714)

Next additions are ordered by evidence value: deeper venue-specific public
liquidation coverage, larger verified weather manifests, and point-in-time
market identity joins. A new case is accepted only after its inputs, costs,
invalidation rule and replay window are recorded.
