use std::sync::Arc;

use prometheus::{Encoder, IntCounter, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};

#[derive(Clone)]
pub struct AppMetrics {
    pub books: crate::book_integrity::BookRegistry,
    pub ingestion: crate::ingestion::IngestionState,
    pub aggregator_retained_bytes: IntGauge,
    pub aggregator_capacity_evictions_total: IntCounter,
    pub pipeline_queue_events: prometheus::IntGaugeVec,
    pub redis_batch_seconds: prometheus::Histogram,
    pub pipeline_latency_seconds: prometheus::HistogramVec,
    registry: Registry,
    pub ticks_ingested_total: IntCounter,
    pub bus_publish_total: IntCounter,
    pub events_ingested_total: IntCounterVec,
    pub bus_events_published_total: IntCounterVec,
    pub ws_subscribers: IntGauge,
    pub redis_xadd_total: IntCounter,
    pub outbox_committed_total: IntCounter,
    pub redis_delivery_failures_total: IntCounter,
    pub redis_dead_letter_total: IntCounter,
    pub redis_dead_letter_failures_total: IntCounter,
    pub redis_accepted_total: IntCounter,
    pub redis_pending_events: IntGauge,
    pub redis_lagged_total: IntCounter,
    pub ticks_dropped_total: IntCounter,
}

impl AppMetrics {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();

        let aggregator_retained_bytes=IntGauge::new("aggregator_retained_bytes","Accounted owned aggregator state allocations, excluding allocator/hash control overhead").expect("valid aggregator gauge");
        let aggregator_capacity_evictions_total=IntCounter::new("aggregator_capacity_evictions_total","Aggregator symbol histories evicted by global budgets; marks source quality incomplete").expect("valid eviction counter");
        registry
            .register(Box::new(aggregator_retained_bytes.clone()))
            .expect("distinct aggregator gauge");
        registry
            .register(Box::new(aggregator_capacity_evictions_total.clone()))
            .expect("distinct eviction counter");
        let pipeline_queue_events = prometheus::IntGaugeVec::new(
            Opts::new(
                "pipeline_queue_events",
                "Observed queue occupancy at router batch boundaries",
            ),
            &["queue"],
        )
        .expect("valid queue metric");
        let redis_batch_seconds = prometheus::Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "redis_durable_batch_seconds",
                "Redis transaction and required local AOF verification duration",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 3.0]),
        )
        .expect("valid durability histogram");
        registry
            .register(Box::new(pipeline_queue_events.clone()))
            .expect("distinct queue metric");
        registry
            .register(Box::new(redis_batch_seconds.clone()))
            .expect("distinct Redis histogram");
        let pipeline_latency_seconds = prometheus::HistogramVec::new(
            prometheus::HistogramOpts::new("pipeline_latency_seconds","Monotonic normalized admission-to-stage completion latency; excludes connector time before emit")
                .buckets(vec![0.00001,0.00005,0.0001,0.0005,0.001,0.005,0.01,0.05,0.1,0.5,1.0,5.0]),
            &["stage","domain"]).expect("valid bounded pipeline histogram");
        registry
            .register(Box::new(pipeline_latency_seconds.clone()))
            .expect("distinct pipeline histogram");
        let ticks_ingested_total = IntCounter::new("ticks_ingested_total", "Total ingested ticks")
            .expect("ticks_ingested_total metric definition must be valid");
        let bus_publish_total = IntCounter::new(
            "bus_publish_total",
            "Total normalized events published to bus",
        )
        .expect("bus_publish_total metric definition must be valid");
        let events_ingested_total = IntCounterVec::new(
            Opts::new(
                "events_ingested_total",
                "Total ingested events by normalized event type",
            ),
            &["event_type"],
        )
        .expect("events_ingested_total metric definition must be valid");
        let bus_events_published_total = IntCounterVec::new(
            Opts::new(
                "bus_events_published_total",
                "Total bus-published events by normalized event type",
            ),
            &["event_type"],
        )
        .expect("bus_events_published_total metric definition must be valid");
        let ws_subscribers = IntGauge::new("ws_subscribers", "Current websocket subscribers")
            .expect("ws_subscribers metric definition must be valid");
        let redis_xadd_total = IntCounter::new("redis_xadd_total", "Total redis xadd writes")
            .expect("redis_xadd_total metric definition must be valid");
        let outbox_committed_total = IntCounter::new(
            "outbox_committed_total",
            "Events committed to durable outbox before publication",
        )
        .expect("valid outbox counter");
        let redis_delivery_failures_total = IntCounter::new(
            "redis_delivery_failures_total",
            "Failed durable Redis delivery or verification attempts",
        )
        .expect("valid Redis failure counter");
        let redis_dead_letter_total = IntCounter::new(
            "redis_dead_letter_total",
            "Total redis sink events synced to the dead-letter file (including shutdown)",
        )
        .expect("redis_dead_letter_total metric definition must be valid");
        let redis_dead_letter_failures_total = IntCounter::new(
            "redis_dead_letter_failures_total",
            "Failed Redis dead-letter batch writes",
        )
        .expect("valid Redis dead-letter failure counter");
        let redis_accepted_total = IntCounter::new(
            "redis_accepted_total",
            "Events accepted into the Redis local queue",
        )
        .expect("valid Redis acceptance counter");
        let redis_pending_events = IntGauge::new(
            "redis_pending_events",
            "Accepted Redis events awaiting Redis acknowledgement or dead-letter sync",
        )
        .expect("valid Redis pending gauge");
        let redis_lagged_total = IntCounter::new(
            "redis_lagged_total",
            "Bus events skipped by the Redis subscriber; any skip fails the sink",
        )
        .expect("valid Redis lag counter");
        let ticks_dropped_total =
            IntCounter::new("ticks_dropped_total", "Total ticks dropped by backpressure")
                .expect("ticks_dropped_total metric definition must be valid");

        registry
            .register(Box::new(ticks_ingested_total.clone()))
            .expect("ticks_ingested_total registration must not conflict");
        registry
            .register(Box::new(bus_publish_total.clone()))
            .expect("bus_publish_total registration must not conflict");
        registry
            .register(Box::new(events_ingested_total.clone()))
            .expect("events_ingested_total registration must not conflict");
        registry
            .register(Box::new(bus_events_published_total.clone()))
            .expect("bus_events_published_total registration must not conflict");
        registry
            .register(Box::new(ws_subscribers.clone()))
            .expect("ws_subscribers registration must not conflict");
        registry
            .register(Box::new(redis_xadd_total.clone()))
            .expect("redis_xadd_total registration must not conflict");
        registry
            .register(Box::new(redis_dead_letter_total.clone()))
            .expect("redis_dead_letter_total registration must not conflict");
        for metric in [
            &outbox_committed_total,
            &redis_delivery_failures_total,
            &redis_dead_letter_failures_total,
            &redis_accepted_total,
            &redis_lagged_total,
        ] {
            registry
                .register(Box::new(metric.clone()))
                .expect("distinct Redis counter");
        }
        registry
            .register(Box::new(redis_pending_events.clone()))
            .expect("distinct Redis gauge");
        registry
            .register(Box::new(ticks_dropped_total.clone()))
            .expect("ticks_dropped_total registration must not conflict");

        Arc::new(Self {
            books: Default::default(),
            ingestion: crate::ingestion::IngestionState::default(),
            pipeline_latency_seconds,
            pipeline_queue_events,
            aggregator_retained_bytes,
            aggregator_capacity_evictions_total,
            redis_batch_seconds,
            registry,
            ticks_ingested_total,
            bus_publish_total,
            events_ingested_total,
            bus_events_published_total,
            ws_subscribers,
            redis_xadd_total,
            outbox_committed_total,
            redis_delivery_failures_total,
            redis_dead_letter_total,
            redis_dead_letter_failures_total,
            redis_accepted_total,
            redis_pending_events,
            redis_lagged_total,
            ticks_dropped_total,
        })
    }

    pub fn render(&self) -> String {
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let _ = encoder.encode(&metric_families, &mut buffer);
        String::from_utf8(buffer).unwrap_or_default()
    }
}
