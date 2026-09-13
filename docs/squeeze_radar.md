# Squeeze Radar v0

`Squeeze Radar v0` is a read-only research application built on top of
MarketBridge's normalized live market data. It ranks observed perpetual
markets by a short-squeeze hypothesis; it does not send orders, authenticate
to an exchange account, sign a wallet, borrow assets, or assert profitability.

## Evidence and states

For each `exchange + symbol` observed by this process, the radar keeps bounded
in-memory observations of funding, OI, price, matched spot/perp CVD, perpetual
best-level OFI and liquidations. It emits one of these states:

```text
stale_data -> data older than the requested bound
warming_up -> a fixed-window or flow baseline is unavailable
watch -> fresh but weak confluence
armed_research_candidate -> fresh, complete data and medium confluence
triggered_research_candidate -> fresh, complete evidence, strong confluence and positive 15m price confirmation
```

`triggered_research_candidate` means “archive and evaluate this observable
setup,” never “buy now.”

| Condition | Points |
| --- | ---: |
| Funding <= -0.05% | 2 |
| Funding < 0 | 1 |
| OI 1h >= 15% | 3 |
| OI 1h >= 3% | 1 |
| Spot CVD up while perp CVD down | 2 |
| Positive 1m OFI | 1 |
| Recent buy liquidation | 1 |
| Positive 15m price change | 1 |

The score maximum is 10. These are v0 research defaults, not universal market
constants. Funding must be compared inside a venue and funding interval; v0
does not normalize venue caps or schedules.

## Run a bounded live study

Start from `config.squeeze-radar.example.yaml`; it watches only BTC, ETH, and
SOL on Binance and OKX. Do not convert broad discovery results directly into
thousands of live subscriptions.

```powershell
$env:MARKETBRIDGE_CONFIG = "config.squeeze-radar.example.yaml"
cargo run
```

At startup rolling fields are intentionally null. Wait a full hour before an
OI-1h metric can exist. A restart clears v0's in-memory history; retain data in
the data lake and use replay for durable studies.

## Discover, scan, and archive

Discover a bounded manual universe:

```powershell
curl "http://127.0.0.1:8080/v1/market/perpetual-funding?exchanges=binance,okx&quote=USDT&limit=50000"
```

Scan only data observed by the current process:

```powershell
curl "http://127.0.0.1:8080/v1/research/squeeze/scan?max_data_age_ms=3000&minimum_score=0&limit=50"
curl "http://127.0.0.1:8080/v1/research/squeeze/scan?exchange=binance&max_data_age_ms=3000"
```

Archive an immutable evidence snapshot before paper analysis:

```powershell
curl -X POST "http://127.0.0.1:8080/v1/research/squeeze/archive?exchange=binance&max_data_age_ms=3000&minimum_score=0"
```

Archives use the local research-workspace `squeeze-scans` namespace and contain
the model version, raw state, rolling timestamps, score evidence, data-quality
gates, and explicit missing evidence. A later scan cannot alter an archive.

## Research acceptance rules

Only use a record for a paper experiment when `fresh` and `ready_for_trigger`
are both true, the rolling baseline elapsed times are reviewed, and all fields
in `missing_evidence` are considered. Define paper entry/exit from observable
bid/ask depth, fees, slippage, funding and latency — never a mid-price fill.

Before asserting an edge, collect a pre-declared universe including failures,
archive every decision, replay without future data, include post-cost outcomes,
and reserve a later time period for holdout validation.

## Explicit v0 gaps

There is no verified circulating-market-cap registry, OI/market-cap factor,
deposit/withdrawal status, borrow availability, exchange reserve measurement,
wallet clustering, holder concentration, liquidation-wall data, or directional
execution model. These fields must remain absent until backed by source
identity, timestamp, coverage, and quality metadata.
