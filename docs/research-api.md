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
  replay engine or a paper portfolio ledger.

Bodies are limited by Axum's JSON body limit (2 MiB); model bounds are 200 levels
per side, 32 sizes per frame and 512 replay frames. Cost work is pure and bounded.
At most two replay workers run concurrently; additional work receives HTTP 429.

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
assumptions. No maker queue simulation, inventory checks, borrow eligibility,
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
