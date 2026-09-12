# Multi-market research platform roadmap

MarketBridge is a multi-market, multi-venue market-data and strategy-research
foundation: live/history ingestion, opportunity scanning, cost analysis,
replay and paper validation through APIs. It does not place orders.

Infrastructure is generic; asset equivalence, conversion, hedging, execution
and cost assumptions belong to each market/strategy model. Crypto, equities,
ETFs, commodities, futures and tokenized assets are in scope. Missing reliable
prices, history or conversion terms downgrade a result, not the product scope.

## Milestones and release gates

- [ ] M0: trustworthy clocks, freshness, observation identity/quality, sized costs.
- [ ] M1: source capabilities, quota/recovery, explicit gaps, durable recording.
- [ ] M2: comparable books, capacity curves, explainable opportunity API/workbench.
- [ ] M3: declarative sources, events and validated configuration reload.
- [ ] M4: deterministic replay, as-of features, paper ledger, reproducible runs.
- [ ] M5: typed SDKs, research templates, deployment, compatibility and recovery.

Each meaningful increment receives a separate tested commit and a development
log entry. A milestone is complete only when its acceptance evidence exists.
Fixture tests, local runtime, live-provider observation, remote CI and production
certification are separate evidence levels. No release tag solely to label
unfinished work as complete. No order/signing/wallet/private trading integration.

## Required invariants

- Wall-clock timestamps are not unique IDs; duration timers use monotonic time.
- Idle quotes expire at read/scan time; unknown/future timestamps are suspect.
- Reference/synthetic/size-specific directional quotes are not executable BBOs.
- Match the same base quantity (or explicit hedge ratio) on both legs.
- Unknown costs stay unknown; quote units and fee assumptions are explicit.
- Rank after costs; preserve residual positions, gaps and failed scenarios.
- History features may use only information known at the decision time.
- Retain source, configuration and model versions for reproduction.

## Evidence log

See [development log](development-log.md). Current package version remains the
last published version until a release candidate passes its documented gates.
Use the [release checklist](release-checklist.md) for candidate-specific evidence.
