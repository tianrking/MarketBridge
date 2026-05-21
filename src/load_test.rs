use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::time::timeout;

use crate::event_bus::EventBus;
use crate::types::{DataEvent, MarketKind, MarketTick, now_ms};

const DEFAULT_EVENTS: u64 = 100_000;
const DEFAULT_SUBSCRIBERS: u64 = 8;
const DEFAULT_BROADCAST_CAPACITY: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadTestConfig {
    pub events: u64,
    pub subscribers: u64,
    pub broadcast_capacity: usize,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            events: DEFAULT_EVENTS,
            subscribers: DEFAULT_SUBSCRIBERS,
            broadcast_capacity: DEFAULT_BROADCAST_CAPACITY,
        }
    }
}

pub fn load_test_config_from_args(args: &[String]) -> Option<LoadTestConfig> {
    if args.get(1).is_none_or(|arg| arg != "load-test") {
        return None;
    }
    let mut cfg = LoadTestConfig::default();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--events" => {
                if let Some(value) = args.get(i + 1).and_then(|x| x.parse::<u64>().ok()) {
                    cfg.events = value.max(1);
                }
                i += 2;
            }
            "--subscribers" => {
                if let Some(value) = args.get(i + 1).and_then(|x| x.parse::<u64>().ok()) {
                    cfg.subscribers = value.max(1);
                }
                i += 2;
            }
            "--broadcast-capacity" => {
                if let Some(value) = args.get(i + 1).and_then(|x| x.parse::<usize>().ok()) {
                    cfg.broadcast_capacity = value.max(1);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(cfg)
}

pub async fn run_load_test(cfg: LoadTestConfig) {
    let bus = EventBus::new(cfg.broadcast_capacity, 1_000);
    let received = Arc::new(AtomicU64::new(0));
    let lagged = Arc::new(AtomicU64::new(0));
    let mut tasks = Vec::new();

    for _ in 0..cfg.subscribers {
        let mut rx = bus.subscribe_events();
        let received = received.clone();
        let lagged = lagged.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(_) => {
                        received.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        lagged.fetch_add(skipped, Ordering::Relaxed);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
    }

    let started = Instant::now();
    for i in 0..cfg.events {
        bus.publish_shared_event(Arc::new(DataEvent::Tick(MarketTick {
            exchange: "synthetic",
            market: MarketKind::Perp,
            symbol: if i % 2 == 0 {
                "BTCUSDT".into()
            } else {
                "ETHUSDT".into()
            },
            bid: 100_000.0 + i as f64,
            ask: 100_000.5 + i as f64,
            mark: Some(100_000.25 + i as f64),
            funding_rate: Some(0.0001),
            ts_ms: now_ms(),
        })));
    }
    let publish_elapsed = started.elapsed();

    let expected = cfg.events.saturating_mul(cfg.subscribers);
    let wait_started = Instant::now();
    let _ = timeout(Duration::from_secs(3), async {
        while received.load(Ordering::Relaxed) < expected {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let total_elapsed = started.elapsed();

    for task in tasks {
        task.abort();
    }

    let published_per_sec = cfg.events as f64 / publish_elapsed.as_secs_f64().max(0.001);
    let delivered = received.load(Ordering::Relaxed);
    let delivered_per_sec = delivered as f64 / total_elapsed.as_secs_f64().max(0.001);

    println!(
        "{}",
        serde_json::json!({
            "mode": "synthetic_load_test",
            "events_published": cfg.events,
            "subscribers": cfg.subscribers,
            "broadcast_capacity": cfg.broadcast_capacity,
            "subscriber_deliveries_expected": expected,
            "subscriber_deliveries_observed": delivered,
            "subscriber_lagged_events": lagged.load(Ordering::Relaxed),
            "publish_elapsed_ms": publish_elapsed.as_millis(),
            "subscriber_wait_elapsed_ms": wait_started.elapsed().as_millis(),
            "total_elapsed_ms": total_elapsed.as_millis(),
            "publish_events_per_sec": published_per_sec,
            "delivered_messages_per_sec": delivered_per_sec,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_load_test_args() {
        let args = vec![
            "market-bridge".to_string(),
            "load-test".to_string(),
            "--events".to_string(),
            "42".to_string(),
            "--subscribers".to_string(),
            "3".to_string(),
        ];

        let cfg = load_test_config_from_args(&args).expect("load-test mode should parse");
        assert_eq!(cfg.events, 42);
        assert_eq!(cfg.subscribers, 3);
    }
}
