# MarketBridge Performance Review

This review records the current performance posture and the next architecture
optimizations. It is intentionally conservative: do not optimize by fabricating
data, removing freshness checks, or letting one slow consumer affect other
consumers.

## Current Hot Path

```text
connector -> SourceRuntime mpsc -> EventRouter
          -> EventBus domain broadcast + DashMap latest snapshots
          -> SpreadAggregator / OrderFlow / Klines / optional Redis
          -> REST + WebSocket consumers
```

Current strengths:

- Latest-state caches use `DashMap`, so high-frequency upserts are in-place and
  do not clone a whole map.
- Router snapshot publishing is offloaded to a bus worker instead of doing all
  cache/broadcast work inline with source receive.
- EventBus has per-domain broadcast channels for funding, OI, trades,
  liquidations, books, and external signals.
- EventBus publishes `SharedEvent` objects for non-quote event domains. Each
  shared event carries both `Arc<DataEvent>` and one pre-serialized JSON payload
  so WebSocket and Redis consumers do not repeatedly serialize the same event.
- Redis uses a local drain channel plus batched `XADD` pipeline and JSONL
  dead-letter writes.
- WebSocket sends are bounded by `runtime.ws_send_timeout_ms`, so one slow
  client is disconnected instead of blocking everyone else.

## Not Yet At The Limit

The current architecture is good for a single-node research data plane, but it
is not the final ceiling. The next bottlenecks are no longer obvious lock
contention; they are allocation, cloning, serialization, and duplicated
per-subscriber snapshot work.

Expected next bottlenecks under higher load:

| Area | Current behavior | Why it becomes expensive |
|---|---|---|
| Router fanout | Router clones each `DataEvent` once so the aggregator can own one copy while the bus owns an `Arc<DataEvent>`. | Large `OrderBook` events still clone level vectors once on the hot path. |
| EventBus fanout | Non-quote event domains use `SharedEvent` and avoid one extra bus-level event clone. | Good first step; quote envelopes and legacy ticks still have their own paths. |
| WebSocket events | Non-quote domain streams reuse the `SharedEvent` JSON payload. | Quote envelopes and snapshot streams still serialize per subscriber. |
| Options/prediction snapshot streams | Each subscriber periodically scans cache and serializes snapshots. | CPU and allocations scale linearly with subscriber count. |
| Redis payload | Redis sink reuses `SharedEvent` JSON for event payloads. | Still copies the string into the Redis row; pipeline I/O remains the main Redis cost. |
| Snapshot keys | Hot updates allocate formatted `String` keys. | Moderate cost at high tick rates; not the first bottleneck. |

## Priority Optimizations

### P0: Arc Event Pipeline

Change router and bus handoff from owned `DataEvent` to shared
`Arc<DataEvent>` for the bus path.

Target:

```text
source event -> Arc<DataEvent>
             -> bus publish without cloning payload
             -> aggregator receives owned event or Arc depending on later refactor
```

Status:

- First pass implemented for the bus path: router sends `Arc<DataEvent>` to the
  bus worker and the aggregator still receives the owned event.

Expected benefit:

- Removes the largest avoidable clone for `OrderBook` and other vector-heavy
  events.
- Lowers heap churn during high-frequency L2 feeds.

Risk:

- Medium. The aggregator currently owns `DataEvent`, so either it keeps one
  clone or moves to `Arc<DataEvent>` as a second step.

### P0: Pre-Serialized Broadcast Payloads

Introduce a small internal wrapper:

```rust
struct SharedEvent {
    event: Arc<DataEvent>,
    json: Arc<str>,
}
```

Use it for WebSocket and Redis fanout where the exact event JSON is needed.

Status:

- First pass implemented for non-quote event domains through `SharedEvent`.
  WebSocket domain streams and Redis reuse the same serialized event payload.

Expected benefit:

- WebSocket CPU becomes closer to `event_count + subscriber_send_count` instead
  of `event_count * subscriber_count * serialize_cost`.
- Redis can reuse the same JSON string for payload.

Risk:

- Medium. Filters still need structured event access, so the wrapper must carry
  both `event` and `json`.

### P0: Shared Snapshot Broadcaster

For `options_chain` and `prediction_book`, replace per-connection cache scans
with one periodic broadcaster per snapshot domain.

Current:

```text
N websocket clients -> N cache scans -> N full serialization passes
```

Target:

```text
one snapshot task -> domain broadcast -> clients filter/send
```

Expected benefit:

- Large improvement when multiple clients subscribe to options or Polymarket
  snapshot streams.
- Prevents a front-end dashboard swarm from multiplying cache reads.

Risk:

- Medium. Need to preserve `include_stale`, symbol, exchange, and product
  filters at the client edge.

### P1: Backpressure By Domain

Order books and trades are high-volume and lossy by nature for latest-state
research, while funding/OI/liquidations are lower-volume and more important to
retain.

Target:

- Separate queue or drop policy by domain.
- Drop-oldest/latest-state policy for book updates.
- Stricter retention for funding/OI/liquidation.

Expected benefit:

- A burst of book updates cannot delay rarer semantic events.

Risk:

- Medium to high. Needs clear semantics so downstream users know which domains
  are loss-tolerant.

### P1: Sharded EventBus

Shard high-volume event broadcast by symbol or `exchange:symbol`.

Expected benefit:

- More parallelism for hot symbols.
- Less single broadcast channel contention under many producers/subscribers.

Risk:

- Medium. Subscription filtering becomes more complex.

### P1: Load Test Harness

Add a synthetic source mode that emits controlled quotes/books/trades without
network I/O.

Measure:

- events/sec in
- events/sec published
- p50/p95/p99 router latency
- WS clients before lag
- Redis batch throughput
- RSS memory

This is required before claiming industrial capacity numbers.

## Current Practical Capacity Estimate

Without a dedicated load test, treat these as engineering estimates only:

| Use case | Current posture |
|---|---|
| Single-user or small research team | Good. |
| Many REST dashboard readers | Good for latest snapshots; heavy full-book requests still clone book vectors. |
| 10-50 WebSocket subscribers on filtered quote/trade domains | Reasonable, depending on symbol filters and source count. |
| Many subscribers to options/prediction snapshots | Needs shared snapshot broadcaster. |
| Redis archival under bursts | Improved by local drain + pipeline, but still serializes separately. |
| Public multi-tenant service | Needs auth/rate limits plus pre-serialized fanout and load tests first. |

## Recommended Next Build Order

1. Add shared snapshot broadcaster for options and prediction snapshots.
2. Extend pre-serialized payloads to quote envelopes where it has clear value.
3. Add synthetic load generator and document measured throughput.
4. Add API key/rate-limit layer before exposing this as a public multi-tenant
   data service.

## Bottom Line

MarketBridge is not at the performance limit. The current design is already
solid for local and team-level research, but the next serious scalability jump
requires reducing clone/serialization multiplication and centralizing snapshot
broadcasts. Those are architecture-level improvements, not more connector work.
