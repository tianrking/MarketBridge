# Development log

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
