# Research API and evidence contract

This incremental API performs research only. It neither places orders nor proves
that submitted evidence, asset identities or fee assumptions are correct.
The first model is `same-asset-spot/v1`; other relationships can be represented
but are explicitly reference-only until a corresponding model is implemented.

## Run locally (PowerShell)

```powershell
$env:MARKETBRIDGE_CONFIG = "config.research.yaml"
cargo run --locked
```

This configuration binds localhost and starts no external collectors. To enable
authentication set `MARKETBRIDGE_API_KEY` before launch. Do not expose the service
publicly with the demonstration configuration.

`MARKETBRIDGE_API_ADDR` optionally overrides the configured socket address.
No HTTP server is required for deterministic local evaluation:

```powershell
cargo run --locked -- --evaluate examples/research/same-asset.json
# Replay accepts the same {"frames":[...]} document as the HTTP endpoint:
cargo run --locked -- --replay path/to/replay.json
cargo run --locked -- --paper path/to/paper.json
cargo run --locked -- --scan path/to/candidates.json
```

After `cargo build --locked`, run `pwsh -File scripts/Test-ResearchApi.ps1` for
an isolated localhost smoke test. It launches and stops its own process, checks
authentication and model failures, and leaves test logs under `examples/out/`.

```powershell
$body = Get-Content examples/research/same-asset.json -Raw
Invoke-RestMethod http://127.0.0.1:8080/v1/research/evaluate -Method Post -ContentType application/json -Body $body
```

For authenticated service add `-Headers @{ 'x-api-key' = $env:MARKETBRIDGE_API_KEY }`.
The example is synthetic; its 5 bps fees are assumptions, not current venue fees.

## Endpoints

- `POST /v1/research/evaluate`: submit a ScanRequest; receive the versioned cost
  curve and evidence references. Invalid inputs return 422. Missing conditions
  produce `reference_only` with reasons; missing costs are never silently zero.
- `POST /v1/research/evaluate-live`: submit `buy` and `sell` Instrument objects,
  `relationship`, `quantities`, `costs`, `max_age_ms`, `max_skew_ms`. The server
  uses the latest cached spot books matched by exact venue/native symbol, stamps
  the decision time, and invokes the same model. Currently only the Binance
  depth20 and OKX books5 snapshot adapters are promoted as complete observed
  windows; other adapters stay reference-only until their delta handling is
  validated. Missing books return 422, not synthetic prices. Caller-supplied
  asset identity is still an assumption, and legacy source timestamps can be
  local receipt times. This is not exchange-level timing certification.
- `POST /v1/research/replay`: submit `{"frames":[<ScanRequest>, ...]}` sorted by
  `as_of_ms`. Future-received observations or future-known relationships fail
  validation. This is deterministic scenario replay, not yet a full tick dataset
  replay engine.
- `POST /v1/research/paper`: submit the prefunded fill scenario described below.
  Uses the same evidence validation as replay, then records inventory, cash costs,
  matched/partial fills and remaining base exposure. Invalid requests return 422.

Bodies are limited by Axum's JSON body limit (2 MiB); model bounds are 200 levels
per side, 32 sizes per frame and 512 replay frames. Cost work is pure and bounded.
Replay, paper and batch scans share at most two concurrent workers; additional work receives
HTTP 429.

The common base identity, quote identity, units, product, chain, issuer and
settlement must agree for the same-asset spot model. Display tickers alone do not
establish equivalence. Futures, options, cross-chain conversions, issuer spreads
and correlated pairs stay in product scope but need their own model adapters.

`complete` means a complete **observed depth window**, not infinite market depth.
Amounts beyond the supplied depth are unavailable, not extrapolated. An input
book must contain finite positive sorted levels and cannot be locally crossed.

`other_cost_quote` is a total cost applied separately to every requested size.
It is not multiplied by quantity. Explicit zero means that this scenario omits
other costs; document that choice under your cost version. Fees use taker
assumptions. The cost-curve endpoint implies no maker queue simulation, inventory checks, borrow eligibility,
currency conversion or simultaneous fill guarantee is implied. Price impact
already included by integrating book levels must not be subtracted twice.

Outputs are floating-point research estimates, not settlement-grade accounting.
Persist requests with results to reproduce a run, including model and cost
versions. The server never substitutes current market data into supplied history.

## Python client

```python
import json, sys
sys.path.insert(0, "sdk/python")
from marketbridge import MarketBridge
with open("examples/research/same-asset.json", encoding="utf-8") as f:
    evidence = json.load(f)
client = MarketBridge()
result = client.evaluate(evidence)
replayed = client.replay([evidence])
```

The current client uses Python's standard library. Typed models, async streaming,
cursor recovery and Arrow/Polars conversion remain planned, not advertised as
already available.

## Batch candidate screening

`POST /v1/research/scan` accepts:

```powershell
$evidence = Get-Content examples/research/same-asset.json -Raw | ConvertFrom-Json
$batch = @{ as_of_ms=$evidence.as_of_ms; min_net_bps=0; candidates=@(@{id="route-a"; evidence=$evidence}) }
Invoke-RestMethod http://127.0.0.1:8080/v1/research/scan -Method Post -ContentType application/json -Body ($batch | ConvertTo-Json -Depth 30)
```

Supply 1..64 uniquely named candidates with a common decision time. Each is evaluated independently;
invalid or reference-only rows remain visible. `ranking` includes only positive
conditional net results satisfying `min_net_bps`, sorted by net bps with stable
ID/size tie-breaking. Thresholds cannot turn unknown costs into valid estimates.
All candidate curves are retained, including rejected and negative results.

`POST /v1/research/scan-live` accepts `min_net_bps` plus `candidates` containing
`id` and `route`; each route has the same fields as `evaluate-live`. One cached
observation per venue/native symbol is used within a request. This is not an
atomic cross-venue snapshot. Missing books become per-candidate errors, not
invented quotes. Scanning is request-driven, not a background alert subscription.

Python: `client.scan(candidates, as_of_ms=10020)` or
`client.scan_live(live_candidates)`. Ranking is not a capital allocation model:
sizes/routes may share liquidity and cannot be added into total profit. Quote
currency identities remain attached; no implicit FX conversion occurs.

Optional public-source observation (starts network collectors):

```powershell
$env:MARKETBRIDGE_CONFIG = "config.research-live.yaml"
cargo run --locked
# In another terminal, after books arrive:
$body = Get-Content examples/research/scan-live.json -Raw
Invoke-RestMethod http://127.0.0.1:8080/v1/research/scan-live -Method Post -ContentType application/json -Body $body
```

The example enables only BTC spot on Binance and OKX; it does not limit platform
asset coverage. It deliberately leaves costs null and relationship `known_at_ms`
zero, so results are reference-only until these assumptions are independently
documented and supplied. Add the API-key header when authentication is enabled.
Provider/network availability is separate from local HTTP test success. Do not
substitute another origin or rotate proxies to bypass provider restrictions.

For a bounded diagnostic run, `pwsh -File scripts/Test-PublicResearchSources.ps1`
starts its own authenticated server, observes for 20 seconds and stops that
process. It reports sampled availability and distinct observation IDs, not a
profitability verdict. Logs remain in `examples/out/`. It is deliberately not
part of deterministic CI and is not a substitute for a long-running soak.

## Prefunded paper scenarios

Build a request from the shipped synthetic evidence fixture:

```powershell
$evidence = Get-Content examples/research/same-asset.json -Raw | ConvertFrom-Json
$paper = @{
  initial = @{ buy_venue_quote=1000000; buy_venue_base=0; sell_venue_quote=0; sell_venue_base=10 }
  frames = @(@{ evidence=$evidence; size_index=0; buy_fill_fraction=1; sell_fill_fraction=0 })
}
Invoke-RestMethod http://127.0.0.1:8080/v1/research/paper -Method Post -ContentType application/json -Body ($paper | ConvertTo-Json -Depth 30)
```

Python: `client.paper(initial_dict, frames_list)`. CLI `--paper` accepts the same JSON.
This example buys 0.5 base units and sells none: residual exposure is 0.5 and
`closed_base_cash_pnl_quote` is null, not a misleading profit number.

Each run uses a fixed instrument pair and explicit initial balances. Fill fractions
are caller-selected scenarios in [0,1], not a prediction of execution. Frames
without a supported cost estimate create no fills. Insufficient inventory or
remaining observed depth skips the whole frame. No implicit borrowing occurs.
Fees apply to the simulated filled notional. `other_cost_quote` is charged once
per frame with any fill, to the buy-venue quote balance; document this allocation.

Reusing the same observation ID consumes its remaining depth instead of resetting
it; reusing an ID with different content fails. A new ID resets available depth,
so this is not counterfactual market-impact simulation. Cash change alone is not
PnL while base exposure is open. Even with zero residual base, cash PnL excludes
unmodeled custody, transfer, opportunity and counterparty risks. This is not yet
a general multi-asset portfolio, mark-to-market, margin or automatic exit engine.

## Optional normalized-event recording

Set `MARKETBRIDGE_RECORD_DIR` to a dedicated output directory before starting a
collector configuration. The bounded writer records arrival sequence, receipt
time, cumulative source drops and normalized payloads. It does **not** preserve
all original exchange frames, and cannot reconstruct information discarded by
legacy connectors. A source drop count means the session is not lossless.

Each session uses create-new filenames, CRC32 corruption checks, a terminal
marker and a `.partial` to `.jsonl` seal after flush/sync. Partial files are never
promoted automatically after a crash. CRC32 is not an authenticity signature.
The current session cap is 256 MiB; a write failure or limit cancels collection.
No automatic deletion, retention rotation or silent history overwrite occurs.
The queue applies backpressure and can affect ingestion latency; benchmark before
using high-volume recording. Power-loss durability is only promised after seal.

```powershell
cargo run --locked -- --verify-journal data/recordings/session-EXAMPLE.jsonl
```

Verification fails on corruption/truncation and reports `sealed` plus source
drops. A sealed file proves a well-formed local recording, not exchange-level
sequence continuity or complete market history.

## Compatibility corrections

- Binance partial spot depth derives its symbol from the combined-stream name;
  the payload does not require `s`. Conflicting payload identity, unexpected stream
  type and malformed levels are rejected. Spot depth still uses receipt time when
  no exchange event timestamp exists. See the
  [official partial-depth specification](https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams).
- `now_ms()` is wall-clock time, not globally unique; timestamps may repeat.
- Quote payloads now include `quote_kind`. Only three explicitly migrated legacy
  adapters (Binance/OKX/Bybit) are classified as observed BBO; others remain
  conservative reference/synthetic data, not forbidden markets.
- Legacy BBO logs are `REFERENCE_ONLY`; they contain no sizes. Legacy depth logs
  use same-base sizing and `net_quote`, but retain an explicit unverified identity
  and continuity label. Maker fee modes do not imply simulated maker fills.
- Historical feature queries with `end_ms` do not receive current funding/OI/book
  context. Correlations align both ends of return intervals within venue/market.
  The existing candle database is not claimed to be a point-in-time revision store.
- Bybit depth now merges snapshot/delta messages, deletes zero-size levels and
  resets on service restart (`u=1`), following the
  [official orderbook protocol](https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook).
  Root source timestamps are retained. Invalid books reset the local builder;
  reconnect starts a new builder. These fixture-tested changes do not certify
  lossless live continuity; Bybit remains reference-only in `evaluate-live`.

## Declarative numeric sources

Existing `aggregates.custom_apis` entries now preserve `name` as `source_instance`
in external signal outputs and storage keys. Optional `timestamp_path` identifies
a JSON integer source time; `timestamp_in_seconds: true` converts seconds to
milliseconds. Receipt time remains separate. Missing/non-finite mapped values or
configured timestamps fail parsing rather than creating successful empty data.

Requests have a 15-second timeout and 1 MiB response cap. Redirects are not
followed. Entries share conservative pacing by origin and share 418/429/503
Retry-After cooldowns (both delta seconds and HTTP dates). This is not a full
account/IP/provider-weight quota model. It does not rotate proxies or promise
zero throttling. Different origins can still share upstream quotas; operators
must respect those provider constraints. Configuration remains startup-only;
hot reload and general event/pagination mappings are not implemented yet.
# Continuous research workspace (unreleased extension)

In addition to the original endpoints below, the current source exposes:

- `POST /v1/research/workspace`: typed action/request API for immutable registries,
  dataset chunks, replay archives, versioned model experiments, announcements,
  cursor reads and integrity checks. See [workspace contract](user-guide/01-workspace.md).
- `GET/POST /v1/research/control`: persisted scanner configuration and status.
  Validation precedes replacement; file reload only covers the research scanner,
  not collector YAML. See [scanner operations](user-guide/04-scanner.md).
- `/workbench`: embedded same-origin research console. Static assets are public;
  data and mutation endpoints remain behind API authentication/rate controls.
- `--replay-journal FILE ROUTE.json`: whole sealed normalized-book file replay;
  strict corruption/drop checks, not raw-exchange replay or simulated execution.

The [complete Chinese manual](user-guide/README.md) covers all nine archived
models, data-source boundaries, historical datasets, allocated portfolios/exits,
announcements, sync/async SDK usage and operational acceptance.
