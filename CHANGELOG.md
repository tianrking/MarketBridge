# Changelog

## v0.0.6 — demand-driven reference data

### Added

- Supply-reference identity mappings now fetch only when an API reader or a
  squeeze scan requests an observed perpetual symbol; fresh snapshots are
  reused within the configured TTL.
- A repository-wide LF text policy prevents Windows, macOS and Linux checkouts
  from introducing noisy line-ending-only changes.

### Corrected

- Strict CI now excludes test-only squeeze and supply helpers from production
  dead-code checks.

## Unreleased — research platform foundation

### Added

- Opt-in background research scanning, durable state-change events, immutable
  configuration revisions and validated scanner JSON hot reload.
- Allocated spot portfolios and exit gates, complete normalized journal-file
  streaming replay, announcement ingestion and descriptive event-window studies.
- Built-in `/workbench` console, async Python client with durable cursor polling,
  model fixture exporter, restart/HTTP tests and bounded soak observation harness.
- Complete Chinese research usage series under `docs/user-guide/`.
- Durable research workspace API: versioned asset relationships, ordered dataset
  chunks, cursor replay, immutable experiment input/output/error archives and CRC.
- Explicit reference-only basis, unit-premium and funding-interval models. Chinese
  usage series starts at `docs/user-guide/README.md`.
- Bounded batch candidate screening, stable after-cost ranking, per-route errors
  and cached-live scanning. Added opt-in public spot observation config/example.
- Prefunded taker paper scenarios with explicit fill fractions, per-venue balances,
  remaining depth accounting and visible unmatched exposure; HTTP/CLI/Python access.
- Explicit instrument identity and relationship evidence for a bounded same-asset
  spot model, cost curves, deterministic scenario replay, CLI and research APIs.
- Live cached-book evaluation with conservative adapter promotion and limitations.
- Opt-in normalized-event journal, independent sequences, CRC verification and
  explicit sealed/partial status; no claim of complete exchange raw history.
- Local-only research configuration, PowerShell HTTP smoke test and dependency-free
  synchronous Python client. See `docs/research-api.md`.

### Corrected

- Updated locked optional quinn-proto to 0.11.15 for GHSA-4w2j-m93h-cj5j;
  the currently selected dependency tree does not activate QUIC.
- Preserve exact floating-point JSON round trips for archived experiment results.
- Live observation exposed Binance spot depth parsing incorrectly requiring payload
  `s`. Partial-depth now uses combined-stream identity, validates conflicting
  identity/levels and preserves exchange event time when available.
- Bybit depth now reconstructs snapshot/delta state, resets on restart and retains
  root message timestamps; fixture coverage does not imply live certification.
- Observation time no longer advances with call count; idle quotes expire at read.
- Legacy depth legs match the same base quantity and rank after costs; symbol-level
  hold timers reset on route changes and use monotonic elapsed time.
- Legacy BBO signals are reference-only; maker fees no longer imply maker fills.
- Historical feature queries no longer receive current snapshot context;
  correlations align actual return intervals within venue/market.
- Empty fee tiers, non-finite settings and invalid source configuration are rejected.
- Independent custom sources retain identity/source time and respect conservative
  shared-origin Retry-After cooldowns, response limits and timeouts.
- Zero-source research mode remains running; closed queues are not silent drops.

### Not yet release-complete

Independent asset attestation, provider-wide weighted quotas, all-venue book reconstruction,
exchange-raw replay, margin/FX/cross-request portfolio models, collector YAML hot reload,
generated SDK types and resumable raw WS histories remain outside this increment. No release tag or
claim of production readiness follows from unit tests alone.
