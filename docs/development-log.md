# Development log

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
