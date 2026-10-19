use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::event_bus::EventBus;
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, MarketTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

const MAX_LATENCY_SAMPLES: u64 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadProfile {
    Quotes,
    Trades,
    Books,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadTestConfig {
    pub events: u64,
    pub subscribers: u64,
    pub broadcast_capacity: usize,
    pub event_bus_shards: usize,
    pub profile: LoadProfile,
    pub symbols: usize,
    pub book_levels: usize,
    pub drain_timeout_ms: u64,
    pub allow_loss: bool,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            events: 100_000,
            subscribers: 8,
            broadcast_capacity: 65_536,
            event_bus_shards: 1,
            profile: LoadProfile::Quotes,
            symbols: 2,
            book_levels: 20,
            drain_timeout_ms: 3_000,
            allow_loss: false,
        }
    }
}

pub fn load_test_config_from_args(args: &[String]) -> Result<Option<LoadTestConfig>> {
    if args.get(1).is_none_or(|arg| arg != "load-test") {
        return Ok(None);
    }
    let mut cfg = LoadTestConfig::default();
    let mut i = 2;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag == "--allow-loss" {
            cfg.allow_loss = true;
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .with_context(|| format!("missing value for {flag}"))?;
        match flag {
            "--events" => cfg.events = value.parse().context("invalid --events")?,
            "--subscribers" => cfg.subscribers = value.parse().context("invalid --subscribers")?,
            "--broadcast-capacity" => {
                cfg.broadcast_capacity = value.parse().context("invalid --broadcast-capacity")?
            }
            "--event-bus-shards" => {
                cfg.event_bus_shards = value.parse().context("invalid --event-bus-shards")?
            }
            "--symbols" => cfg.symbols = value.parse().context("invalid --symbols")?,
            "--book-levels" => cfg.book_levels = value.parse().context("invalid --book-levels")?,
            "--drain-timeout-ms" => {
                cfg.drain_timeout_ms = value.parse().context("invalid --drain-timeout-ms")?
            }
            "--profile" => {
                cfg.profile = match value.as_str() {
                    "quotes" => LoadProfile::Quotes,
                    "trades" => LoadProfile::Trades,
                    "books" => LoadProfile::Books,
                    "mixed" => LoadProfile::Mixed,
                    _ => bail!("--profile must be quotes, trades, books or mixed"),
                }
            }
            _ => bail!("unknown load-test option: {flag}"),
        }
        i += 2;
    }
    ensure!(
        (1..=10_000_000).contains(&cfg.events),
        "--events must be 1..10000000"
    );
    ensure!(
        (1..=256).contains(&cfg.subscribers),
        "--subscribers must be 1..256"
    );
    ensure!(
        (1..=1_048_576).contains(&cfg.broadcast_capacity),
        "--broadcast-capacity must be 1..1048576"
    );
    ensure!(
        (1..=64).contains(&cfg.event_bus_shards),
        "--event-bus-shards must be 1..64"
    );
    ensure!(
        (1..=100_000).contains(&cfg.symbols),
        "--symbols must be 1..100000"
    );
    ensure!(
        (1..=200).contains(&cfg.book_levels),
        "--book-levels must be 1..200 per side"
    );
    ensure!(
        (1..=60_000).contains(&cfg.drain_timeout_ms),
        "--drain-timeout-ms must be 1..60000"
    );
    Ok(Some(cfg))
}

#[derive(Default)]
struct SubscriberCounts {
    delivered: AtomicU64,
    lagged: AtomicU64,
}

pub async fn run_load_test(cfg: LoadTestConfig) -> Result<()> {
    let bus = EventBus::new_sharded(cfg.broadcast_capacity, 1_000, cfg.event_bus_shards);
    let mut counters = Vec::new();
    let mut tasks = JoinSet::new();
    for _ in 0..cfg.subscribers {
        // Register before publication, even if this task has not been scheduled yet.
        let mut rx = bus.subscribe_events();
        let counts = Arc::new(SubscriberCounts::default());
        counters.push(counts.clone());
        tasks.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(_) => {
                        counts.delivered.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        counts.lagged.fetch_add(skipped, Ordering::Relaxed);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    let mut stride = cfg.events.div_ceil(MAX_LATENCY_SAMPLES);
    // An odd stride visits all four mixed domains; a multiple of four would
    // otherwise sample only quotes and hide book-publication latency.
    if cfg.profile == LoadProfile::Mixed && stride.is_multiple_of(2) {
        stride += 1;
    }
    let mut latencies = Vec::new();
    let started = Instant::now();
    for i in 0..cfg.events {
        let event = Arc::new(synthetic_event(cfg, i));
        if i % stride == 0 {
            let sample_start = Instant::now();
            bus.publish_shared_event(event);
            latencies.push(sample_start.elapsed().as_nanos() as u64);
        } else {
            bus.publish_shared_event(event);
        }
    }
    let publish_elapsed = started.elapsed();
    // Closing the senders lets every receiver drain and report exact loss. Do not
    // wait for delivered == expected after a lag: skipped messages cannot return.
    drop(bus);
    let wait_started = Instant::now();
    let drained = timeout(Duration::from_millis(cfg.drain_timeout_ms), async {
        while let Some(result) = tasks.join_next().await {
            result?;
        }
        Ok::<(), tokio::task::JoinError>(())
    })
    .await;
    let drain_timed_out = drained.is_err();
    let worker_failed = matches!(drained, Ok(Err(_)));
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let wait_elapsed = wait_started.elapsed();
    let total_elapsed = started.elapsed();

    let delivered: u64 = counters
        .iter()
        .map(|c| c.delivered.load(Ordering::Relaxed))
        .sum();
    let lagged: u64 = counters
        .iter()
        .map(|c| c.lagged.load(Ordering::Relaxed))
        .sum();
    let expected = cfg.events * cfg.subscribers;
    let unaccounted = expected.saturating_sub(delivered.saturating_add(lagged));
    let complete = !drain_timed_out && !worker_failed && delivered + lagged == expected;
    let passed = complete && (cfg.allow_loss || lagged == 0);
    latencies.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "mode": "synthetic_load_test",
            "scope": "event_bus_and_latest_snapshots_not_source_router_network_or_storage",
            "profile": cfg.profile,
            "symbols": cfg.symbols,
            "book_levels_per_side": cfg.book_levels,
            "events_published": cfg.events,
            "subscribers": cfg.subscribers,
            "broadcast_capacity": cfg.broadcast_capacity,
            "event_bus_shards": cfg.event_bus_shards,
            "subscriber_deliveries_expected": expected,
            "subscriber_deliveries_observed": delivered,
            "subscriber_lagged_events": lagged,
            "subscriber_deliveries_unaccounted": unaccounted,
            "drain_timed_out": drain_timed_out,
            "worker_failed": worker_failed,
            "delivery_accounting_complete": complete,
            "lossless": complete && lagged == 0,
            "allow_loss": cfg.allow_loss,
            "passed": passed,
            "publish_elapsed_ms": publish_elapsed.as_millis(),
            "subscriber_wait_elapsed_ms": wait_elapsed.as_millis(),
            "total_elapsed_ms": total_elapsed.as_millis(),
            "publish_events_per_sec": cfg.events as f64 / publish_elapsed.as_secs_f64().max(0.000_001),
            "delivered_messages_per_sec": delivered as f64 / total_elapsed.as_secs_f64().max(0.000_001),
            "bus_publish_call_latency_us": {
                "samples": latencies.len(),
                "sample_stride": stride,
                "p50": percentile_us(&latencies, 50),
                "p95": percentile_us(&latencies, 95),
                "p99": percentile_us(&latencies, 99),
                "sample_max": latencies.last().copied().unwrap_or(0) as f64 / 1000.0,
            },
        })
    );
    ensure!(
        passed,
        "load test failed: lagged={lagged}, unaccounted={unaccounted}, drain_timed_out={drain_timed_out}, worker_failed={worker_failed}"
    );
    Ok(())
}

fn percentile_us(sorted: &[u64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted.get(index).copied().unwrap_or(0) as f64 / 1000.0
}

fn synthetic_event(cfg: LoadTestConfig, i: u64) -> DataEvent {
    let symbol_index = if cfg.profile == LoadProfile::Mixed {
        i / 4
    } else {
        i
    };
    let symbol = format!("SYNTH{:06}USDT", symbol_index % cfg.symbols as u64).into_boxed_str();
    let ts_ms = now_ms();
    let profile = if cfg.profile == LoadProfile::Mixed {
        [
            LoadProfile::Quotes,
            LoadProfile::Trades,
            LoadProfile::Books,
            LoadProfile::Mixed,
        ][(i % 4) as usize]
    } else {
        cfg.profile
    };
    match profile {
        LoadProfile::Quotes => DataEvent::Tick(MarketTick {
            exchange: "synthetic",
            market: MarketKind::Perp,
            symbol,
            bid: 100.0,
            ask: 100.5,
            mark: Some(100.25),
            funding_rate: Some(0.0001),
            ts_ms,
        }),
        LoadProfile::Trades => DataEvent::Trade(TradeTick {
            exchange: "synthetic",
            market: MarketKind::Perp,
            symbol,
            price: 100.0,
            qty: 1.0,
            side: TradeSide::Buy,
            trade_id: Some(i.to_string().into()),
            ts_ms,
        }),
        LoadProfile::Books => DataEvent::OrderBook(OrderBookTick {
            book_quality: None,
            exchange: "synthetic",
            market: MarketKind::Perp,
            symbol,
            bids: (0..cfg.book_levels)
                .map(|level| BookLevel {
                    price: 100.0 - level as f64 * 0.01,
                    qty: 1.0,
                })
                .collect(),
            asks: (0..cfg.book_levels)
                .map(|level| BookLevel {
                    price: 100.5 + level as f64 * 0.01,
                    qty: 1.0,
                })
                .collect(),
            last_update_id: Some(i),
            ts_ms,
        }),
        LoadProfile::Mixed => DataEvent::FundingRate(FundingRateTick {
            exchange: "synthetic",
            symbol,
            funding_rate: 0.0001,
            next_funding_time_ms: None,
            mark_price: Some(100.25),
            index_price: Some(100.0),
            ts_ms,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        ["market-bridge", "load-test"]
            .into_iter()
            .chain(values.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn parses_load_test_args_and_rejects_invalid_requests() {
        let cfg = load_test_config_from_args(&args(&[
            "--events",
            "42",
            "--subscribers",
            "3",
            "--event-bus-shards",
            "4",
            "--profile",
            "mixed",
            "--symbols",
            "20",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(cfg.events, 42);
        assert_eq!(cfg.subscribers, 3);
        assert_eq!(cfg.event_bus_shards, 4);
        assert_eq!(cfg.profile, LoadProfile::Mixed);
        assert_eq!(cfg.symbols, 20);
        for invalid in [
            &["--events", "0"][..],
            &["--events", "oops"],
            &["--events"],
            &["--profile", "unknown"],
            &["--typo", "1"],
        ] {
            assert!(
                load_test_config_from_args(&args(invalid)).is_err(),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn mixed_profile_exercises_four_valid_domains() {
        let cfg = LoadTestConfig {
            profile: LoadProfile::Mixed,
            symbols: 100,
            ..Default::default()
        };
        assert!(matches!(synthetic_event(cfg, 0), DataEvent::Tick(_)));
        assert!(matches!(synthetic_event(cfg, 1), DataEvent::Trade(_)));
        assert!(matches!(synthetic_event(cfg, 2), DataEvent::OrderBook(_)));
        assert!(matches!(synthetic_event(cfg, 3), DataEvent::FundingRate(_)));
        for i in 0..100 {
            assert!(synthetic_event(cfg, i).has_finite_numbers());
        }
    }

    #[tokio::test]
    async fn complete_small_run_passes_for_each_profile() {
        for profile in [
            LoadProfile::Quotes,
            LoadProfile::Trades,
            LoadProfile::Books,
            LoadProfile::Mixed,
        ] {
            run_load_test(LoadTestConfig {
                events: 16,
                subscribers: 2,
                broadcast_capacity: 32,
                profile,
                ..Default::default()
            })
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn lag_is_failure_unless_explicitly_allowed() {
        // On this single-thread test runtime publication cannot yield to receivers.
        let cfg = LoadTestConfig {
            events: 4096,
            subscribers: 1,
            broadcast_capacity: 1,
            ..Default::default()
        };
        assert!(run_load_test(cfg).await.is_err());
        run_load_test(LoadTestConfig {
            allow_loss: true,
            ..cfg
        })
        .await
        .unwrap();
    }
}
