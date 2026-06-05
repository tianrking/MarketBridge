# Symbol-Level Short-Squeeze And Exhaustion-Short Examples

This guide shows how to use MarketBridge's current data plane to analyze one
symbol and run two read-only example monitors:

- `/v1/research/symbol-state`: an in-service real-time state machine that
  consumes normalized events and exposes current squeeze/exhaustion state.
- `short_squeeze_monitor`: a long-following example for short-squeeze
  confluence.
- `exhaustion_short_monitor`: a short-side example for long-crowding and
  liquidity exhaustion.

MarketBridge remains a data unification layer. It does not place orders, sign
wallets, prove factor validity, run PnL, or manage execution. The state machine
and examples emit read-only metrics, stages, evidence strings, and risk context.

## Available Data Dimensions

| Dimension | Status | Main Interface | Notes |
|---|---|---|---|
| Price and spot-perp basis | Implemented | `/v1/market/quotes`, `/v1/market/basis`, `/v1/market/klines` | Current prices, basis, and candle context. |
| Tick/book/trade/OI storage | Optional | `runtime.clickhouse` | Writes ClickHouse `JSONEachRow` batches into MergeTree tables. |
| Funding rate | Implemented | `/v1/market/funding` | Mostly keyless for perpetual venues; CoinGlass aggregates require a key. |
| Open interest | Implemented | `/v1/market/open-interest` | The examples compute OI change by polling at least twice. |
| CVD and aggressive flow | Implemented | `/v1/market/order-flow`, `/v1/market/footprint` | Derived from normalized live trade events. |
| L2 depth and OFI | Implemented | `/v1/market/order-books`, `/v1/research/symbol-state` | The state machine computes top-depth ratio, depth pressure, and best-level OFI. |
| Liquidations | Partial | `/v1/market/liquidations` | Depends on whether the venue has a stable public liquidation feed. |
| CoinGlass derivatives aggregates | Implemented, keyed | `/v1/external/signals?sources=coinglass` | Funding, OI, liquidations, long-short, basis, and options OI aggregates. |
| On-chain transfers | Implemented, configured | `/v1/onchain/transfers` | Whale Alert and Etherscan require keys; Etherscan is address-watchlist based. |
| Liquidation heatmap price walls | Gap | none | Requires a future heatmap provider or a custom JSON API configured under `aggregates.custom_apis`. |
| Precise whale/team-to-CEX detection | Partial gap | `/v1/onchain/transfers` | Requires exchange address labels, known wallets, or high-quality provider labels. |

## Real-Time Symbol State API

After MarketBridge is running:

```bash
curl -s "http://127.0.0.1:8080/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance" | jq
```

Important response fields:

| Field | Meaning |
|---|---|
| `metrics.funding_rate` | Current funding rate. |
| `metrics.open_interest_change_pct` | Adjacent OI snapshot change while the service is running. |
| `metrics.spot_cvd_notional_1m` | One-minute spot CVD notional. |
| `metrics.perp_cvd_notional_1m` | One-minute perpetual CVD notional. |
| `metrics.cvd_divergence` | CVD divergence label such as `spot_up_perp_down`. |
| `metrics.bid_ask_depth_ratio_10` | Top-10 bid depth divided by top-10 ask depth. |
| `metrics.depth_pressure_10` | `(bid_depth - ask_depth) / total_depth`. |
| `metrics.ofi_best_level_1m` | One-minute best-level OFI notional. |
| `metrics.buy_liquidation_notional_15m` | Buy-side liquidation notional over the last 15 minutes. |
| `long_squeeze.state` | Current long squeeze stage. |
| `short_exhaustion.state` | Current short exhaustion stage. |
| `risk_context` | Read-only risk context; no execution. |

Long squeeze stages:

```text
neutral -> short_crowding -> chip_accumulation -> spot_absorption -> triggered_long_squeeze
```

Short exhaustion stages:

```text
neutral -> long_crowding -> fuel_exhaustion -> book_vacuum -> triggered_short_exhaustion
```

OI change, OFI, and CVD require at least two relevant events or polling cycles.
Immediately after startup, some fields may be `null`; that is expected.

## Example 1: Short-Squeeze Long Follow

The example looks for this confluence:

1. Funding stays deeply negative, for example below `-0.05%`.
2. OI expands while price moves sideways or drifts lower.
3. Spot CVD rises while perpetual CVD falls.
4. Recent liquidation flow or aggregate liquidation data suggests squeeze fuel.

Run:

```bash
cargo run --example short_squeeze_monitor -- --symbol BTCUSDT --exchange binance --iterations 5 --interval-secs 30
```

The output reports a score and evidence lines. OI-change evidence requires at
least two polling iterations because the current REST endpoint returns latest
snapshots, not a historical OI series.

What it can evaluate today:

- whether funding is extremely negative;
- whether OI expands while the service is running;
- whether spot/perp CVD diverges;
- whether native liquidation feeds show recent liquidations;
- whether CoinGlass liquidation aggregates are available.

What it cannot fully evaluate yet:

- exact 50x/100x liquidation wall locations;
- the full heatmap structure behind "where cascading liquidations may occur."

To fill that gap, add a liquidation-heatmap provider and normalize fields such
as `symbol`, `side`, `price_level`, `leverage_band`, `estimated_notional`, and
`ts_ms` into either `external_signal` or a dedicated `liquidation_heatmap`
domain.

## Example 2: Exhaustion Short

The example looks for this confluence:

1. Funding is high and positive, but price fails to hold a breakout.
2. Price pushes to a new high while OI falls, suggesting short-cover fuel is
   getting exhausted.
3. Perp CVD weakens or seller-initiated flow increases.
4. Bid depth is thin relative to ask depth.
5. Whale or team wallets transfer meaningful size toward CEX venues.

Run:

```bash
cargo run --example exhaustion_short_monitor -- --symbol BTCUSDT --exchange binance --iterations 5 --interval-secs 30
```

What it can evaluate today:

- whether funding is long-crowded;
- whether OI drops between polling iterations;
- whether recent one-minute candles show a push-and-fail pattern;
- whether perp CVD weakens;
- whether top-book bid/ask depth leans bearish.

What is only partially available today:

- whale/team transfers to CEX. MarketBridge can store on-chain transfer data,
  but this depends on Whale Alert, an Etherscan watchlist, and address-label
  quality.

Suggested on-chain configuration:

```yaml
onchain:
  whale_alert:
    enabled: true
    api_key_env: WHALE_ALERT_API_KEY
    min_value_usd: 1000000
  etherscan:
    enabled: true
    api_key_env: ETHERSCAN_API_KEY
    min_value_eth: 1000
    addresses:
      - "0x..."
```

## Recommended Single-Symbol Analysis Order

For a symbol such as `BTCUSDT`, `ETHUSDT`, or another perpetual contract, a
typical read-only query sequence is:

1. `/v1/agent/context?symbols=BTCUSDT&include_storage=true`
2. `/v1/research/symbol-state?symbol=BTCUSDT&exchange=binance`
3. `/v1/research/features?symbols=BTCUSDT&exchange=binance&market=perp&intervals=1m,5m,15m`
4. `/v1/market/funding?symbols=BTCUSDT`
5. `/v1/market/open-interest?symbols=BTCUSDT`
6. `/v1/market/order-flow?symbol=BTCUSDT&market=spot&window_ms=60000`
7. `/v1/market/order-flow?symbol=BTCUSDT&market=perp&window_ms=60000`
8. `/v1/market/order-books?symbols=BTCUSDT&market=perp`
9. `/v1/market/liquidations?symbols=BTCUSDT`
10. `/v1/onchain/transfers` and `/v1/external/signals?sources=coinglass`

For production trading systems, keep execution and risk management in a
separate service. MarketBridge's built-in state machine is a read-only analysis
layer and does not cross the data-only boundary.

## ClickHouse Storage

Ticks, books, trades, funding, OI, liquidations, and external signals can be
written to ClickHouse. Start ClickHouse locally:

```bash
docker run --rm -p 8123:8123 -p 9000:9000 --name marketbridge-clickhouse clickhouse/clickhouse-server:latest
```

Enable the sink:

```yaml
runtime:
  clickhouse:
    enabled: true
    url: "http://127.0.0.1:8123"
    database: "marketbridge"
    batch_max: 1000
    flush_ms: 250
    init_tables: true
```

MarketBridge creates:

- `marketbridge.market_quotes`
- `marketbridge.trades`
- `marketbridge.order_books`
- `marketbridge.funding_rates`
- `marketbridge.open_interest`
- `marketbridge.liquidations`
- `marketbridge.external_signals`
