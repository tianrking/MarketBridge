# MarketBridge performance and operational review

Reviewed on 2026-10-19 for ordinary Linux/VPS deployments, with shared Windows
and macOS code. The detailed findings, raw benchmark links, tradeoffs and
remaining priorities are in the [Chinese practical review](practical_runtime_review.zh-CN.md).

MarketBridge has a useful cross-platform research data plane. The available
measurements do not establish an industrial capacity limit, a hard real-time
latency bound or optimality across workloads.

## Changes and evidence

- Reserve the API listener before collectors/recording start; occupied ports
  fail startup with a nonzero status.
- Handle Unix SIGTERM/SIGINT through the common shutdown path and seal normalized
  recordings. Windows retains console Ctrl-C support. Forced termination can
  still leave partial recordings.
- Supervise registered background services in completion order while the process
  is running. An enabled service's unexpected exit fails the process; shutdown
  still waits for other services and preserves task panics.
- Amortize full snapshot-retention scans on a shared one-second schedule, recover
  after clock regression, retain per-update capacity enforcement and calculate
  freshness at read time. Expired memory entries can await the next sweep.
- Update snapshots before notifying event/domain subscribers.
- Pin Rust 1.97.1 across development, CI and release targets; add macOS CI and
  portable process/CLI checks.

Single-shard direct subscription was implemented and measured as a candidate.
Its benefits depended on workload and thread budget, and removing its queue also
changes slow-consumer buffering. Buffered subscriptions remain the default.
The raw candidate and negative results are retained in the detailed review.

## Run useful checks

From the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked
python3 -B scripts/Test-ProcessLifecycle.py
python3 -B -m unittest discover -s sdk/python -p 'test_*.py'
python3 -B scripts/run_python_example_tests.py
```

The local research API uses `config.research.yaml`, SQLite and an embedded
workbench; it does not need Node.js, Redis or market credentials. Unix process
tests verify ordinary signal shutdown and journal sealing. CI configures Linux,
Windows and macOS, with Unix signal tests explicitly skipped on Windows. Only
Linux was executed in this cloud session.

## Benchmark scope and reproduction

```bash
cargo build --release --locked
python3 scripts/benchmark_event_bus.py --label my-build --events 100000 \
  --workers 3 --output examples/out/my-build-benchmark.jsonl
```

Use a new output file each time. The runner records binary hashes, checkout
state, host/toolchain metadata, warmups and repeated raw reports. The CPU budget
must account for the producer's main thread in addition to Tokio workers; the
three-worker example matches the review machine's four-core quota and is not a
universal deployment setting.

Profiles cover quotes, trades, full synthetic books and mixed traffic. Select
`--profile quotes|trades|books|mixed`, `--symbols N`, `--book-levels N` per side,
`--broadcast-capacity N` and `--event-bus-shards N` in the underlying `load-test`
command. Mixed profiles cycle every domain through the requested universe.

The measured scope is EventBus publication, latest snapshots and in-process
subscriber delivery. Throughput includes synthetic event generation;
`bus_publish_call_latency_us` times the synchronous publication call. Neither
measures source/router/network/storage/serialization load or end-to-end latency.
Samples report their count and stride. Mixed sampling visits all four domains.

The producer closes its senders and waits for bounded draining. Delivered,
lagged and unaccounted messages, timeouts and worker failures are reported
separately. Invalid options, incomplete runs and loss fail with a nonzero status.
`--allow-loss` permits explicit overload experiments, reports their loss and
still requires complete accounting.

Burst results cannot establish sustained live-market capacity or p99.9 bounds.
The review includes all workload results and observed regressions, not only the
best rate. Four shards also allocate a larger aggregate broadcast budget than
one shard at the same per-shard capacity; default sharding remains one.

## Remaining work before stronger operational guarantees

1. Subscription gap/epoch metadata now marks order-flow, footprint and strategy
   responses; squeeze v1 rejects known gaps. Source sequence gaps, retention
   eviction markers and validated per-window recovery remain unfinished.
2. Profile the actual trade aggregation path and bound global retained trade
   memory. Its per-event global pruning is outside the EventBus benchmark.
3. Measure journal/router/aggregator queue waits and isolate congestion only
   after defining each domain's retention semantics.
4. Redis now drains locally accepted events on cancellation, bounds network
   waits and propagates dead-letter/lag errors. Durable outbox, deduplication,
   checked replay and full-ingestion shutdown remain unfinished; there is no
   exactly-once or lossless archival guarantee.
5. Add end-to-end monotonic timing and source readiness, then validate a sustained
   workload on the intended VPS and supported OS/runtime-library baselines.

PREEMPT_RT reduces kernel scheduling latency; it does not remove these
application costs or exchange/network delays. No real-time scheduling, affinity,
busy polling or kernel changes are required by this setup. See the detailed
review for the Linux and pinned Tokio documentation used in this assessment.
