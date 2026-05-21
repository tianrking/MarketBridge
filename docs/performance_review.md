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
- EventBus publishes `SharedEvent` and `SharedQuote` objects. JSON payloads are
  generated lazily on first WebSocket/Redis use and then reused, so idle
  research runs do not pay serialization cost just because an event passed
  through the bus.
- Options and prediction snapshot streams use a shared broadcaster and skip
  cache scans when there are no subscribers.
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
| EventBus fanout | Event domains use `SharedEvent`; quote streams use `SharedQuote`. | Aggregator still receives an owned clone, so large L2 books pay one clone before analytics. |
| WebSocket events | Domain and quote streams reuse lazy JSON payloads after the first consumer needs them. | Legacy `/ws/ticks` still serializes per matching subscriber. |
| Options/prediction snapshot streams | One shared broadcaster scans caches and serializes snapshots only while subscribers exist. | Client-side filters still receive the shared stream and drop nonmatching rows. |
| Redis payload | Redis sink reuses lazy `SharedEvent` JSON for event payloads. | Still copies the string into the Redis row; pipeline I/O remains the main Redis cost. |
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
    json: OnceLock<Arc<str>>,
}
```

Use it for WebSocket and Redis fanout where the exact event JSON is needed.

Status:

- Implemented for event domains and market quotes.
- JSON is lazy: it is not generated until a WebSocket or Redis consumer needs
  it, and then it is reused by later consumers.

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

Status:

- Implemented as configurable event/domain broadcast shards through
  `runtime.event_bus_shards`.
- Default remains `1` to keep local research simple. Increase only after load
  tests show one broadcast domain is the bottleneck.

Expected benefit:

- More parallelism for hot symbols.
- Less single broadcast channel contention under many producers/subscribers.

Risk:

- Medium. Subscription filtering becomes more complex.

### P1: Load Test Harness

Add a synthetic source mode that emits controlled quotes/books/trades without
network I/O.

Status:

- Implemented as `market-bridge load-test`.
- Latest local smoke run on this machine:
  - `events_published`: `10000`
  - `subscribers`: `4`
  - `subscriber_deliveries_observed`: `40000`
  - `subscriber_lagged_events`: `0`
  - `publish_events_per_sec`: about `82k`
  - `delivered_messages_per_sec`: about `328k`

Example:

```bash
market-bridge load-test --events 100000 --subscribers 8 --broadcast-capacity 65536
```

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

1. Move aggregator input to `Arc<DataEvent>` or split large L2 books into
   shared payloads so analytics and bus paths do not clone level vectors.
2. Add measured load-test profiles for quote-only, trade-only, order-book, and
   mixed event streams.
3. Use the synthetic load generator in CI/manual release checks and record
   measured throughput per machine class.
4. Increase `runtime.event_bus_shards` only after measured load tests show
   single-domain broadcast contention.

## Bottom Line

MarketBridge is not at the performance limit. The current design is already
solid for local and team-level research, but the next serious scalability jump
requires reducing clone/serialization multiplication and centralizing snapshot
broadcasts. Those are architecture-level improvements, not more connector work.
