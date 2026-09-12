# Development log

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
