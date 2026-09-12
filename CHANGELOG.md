# Changelog

## Unreleased — research platform foundation

### Added

- Explicit instrument identity and relationship evidence for a bounded same-asset
  spot model, cost curves, deterministic scenario replay, CLI and research APIs.
- Live cached-book evaluation with conservative adapter promotion and limitations.
- Opt-in normalized-event journal, independent sequences, CRC verification and
  explicit sealed/partial status; no claim of complete exchange raw history.
- Local-only research configuration, PowerShell HTTP smoke test and dependency-free
  synchronous Python client. See `docs/research-api.md`.

### Corrected

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

Full asset registry, provider-wide weighted quotas, all-venue book reconstruction,
raw-event replay, portfolio paper ledger, event studies, configuration hot reload,
full workbench and async/generated SDKs remain on the roadmap. No release tag or
claim of production readiness follows from unit tests alone.
