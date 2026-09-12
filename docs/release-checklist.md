# Research-platform release checklist

This checklist is not a release announcement. The current work is unreleased.
Mark a gate complete only with evidence from the exact candidate commit.

## Scope and contract

- [ ] Freeze the candidate scope and version; list implemented, partial and
  planned capabilities in `feature_inventory.md`. No implied universal strategy.
- [ ] Reconcile README English/Chinese, API usage, source inventory, architecture,
  examples, SDK methods, changelog and downloadable binary commands.
- [ ] Publish request/response compatibility changes and migration examples,
  particularly evidence identity, time, quote kind and rejection semantics.
- [ ] Confirm no order, signing or wallet feature was introduced.

## Reproducible software evidence

- [ ] Clean candidate checkout passes locked format, clippy, test and build on
  the advertised Windows/Linux targets, plus Python and authenticated HTTP tests.
- [ ] CLI/API output agrees for identical model inputs; tested failure cases
  include missing costs, unknown identity, stale/skewed data and insufficient depth.
- [ ] Check malformed/oversized input, busy workers and authentication failures.
- [ ] Validate journal crash/partial handling, corruption detection, dropped
  counters, storage limits, backup/recovery and retention behavior.
- [ ] Review exact staged changes, dependency/license changes and private material.
- [ ] Packaging includes required configs and current docs; verify checksums and
  run the extracted binary, not only the development build.

## Operational evidence (not replaced by unit tests)

- [ ] Run at least a 72-hour observation on the intended deployment environment
  with the chosen public sources; retain configuration/commit/time evidence.
- [ ] Measure ingestion lag/freshness, parse failures, source and subscriber gaps,
  reconnect/recovery, CPU/memory, queue occupancy and disk growth.
- [ ] Exercise disconnect, source stall, 429/Retry-After, disk-full and restart.
  Never interpret an intentionally quiet/closed market as a universal 3-second
  connection failure, and never claim immunity to upstream throttling.
- [ ] Do not promote a connector to stronger research evidence solely because it
  is wired. Validate snapshot/delta semantics and timestamp precision per source.
- [ ] Verify the terms, attribution, retention and redistribution rights of each
  source before commercial API exposure. Public access is not a resale license.

## Research validity

- [ ] Each advertised model has explicit supported asset relationships, units,
  contract semantics, cost assumptions, fill/exit rules and invalidation reasons.
- [ ] Hold out data by time, prevent future information, keep failed/delisted
  instruments and report missing-data effects. Separate fitted and tested periods.
- [ ] Record sensitivity to latency, costs, depth, partial fills and inventory.
  Report open exposure separately; never aggregate overlapping opportunity sizes.
- [ ] Compare paper results with independent evidence. No promise of realized
  profit follows from a positive conditional estimate or a well-formed ledger.

## Publication

- [ ] Confirm intended remote/branch and user authorization for publication.
- [ ] Verify remote CI for the exact pushed SHA; a local pass is not remote CI.
- [ ] Publish only the validated scope with known limitations and rollback steps.

Research work can proceed incrementally before these release gates pass. A small
release may exclude unfinished features explicitly; it must not claim the full
M0–M5 roadmap is complete merely because a version was assigned.
