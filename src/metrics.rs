use std::sync::Arc;

use prometheus::{Encoder, IntCounter, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};

#[derive(Clone)]
pub struct AppMetrics {
    registry: Registry,
    pub ticks_ingested_total: IntCounter,
    pub bus_publish_total: IntCounter,
    pub events_ingested_total: IntCounterVec,
    pub bus_events_published_total: IntCounterVec,
    pub ws_subscribers: IntGauge,
    pub redis_xadd_total: IntCounter,
    pub redis_dead_letter_total: IntCounter,
    pub ticks_dropped_total: IntCounter,
}

impl AppMetrics {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();

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
        let redis_dead_letter_total = IntCounter::new(
            "redis_dead_letter_total",
            "Total redis sink events moved to dead letter after retries",
        )
        .expect("redis_dead_letter_total metric definition must be valid");
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
        registry
            .register(Box::new(ticks_dropped_total.clone()))
            .expect("ticks_dropped_total registration must not conflict");

        Arc::new(Self {
            registry,
            ticks_ingested_total,
            bus_publish_total,
            events_ingested_total,
            bus_events_published_total,
            ws_subscribers,
            redis_xadd_total,
            redis_dead_letter_total,
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
