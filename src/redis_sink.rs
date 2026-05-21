use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;
use crate::types::{DataEvent, MarketKind};

const XADD_MAX_ATTEMPTS: usize = 8;
const XADD_INITIAL_BACKOFF_MS: u64 = 100;
const XADD_MAX_BACKOFF_MS: u64 = 5_000;
const XADD_BATCH_MAX: usize = 100;
const XADD_BATCH_FLUSH_MS: u64 = 50;
const REDIS_LOCAL_BUFFER: usize = 100_000;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RedisEventRow {
    stream: String,
    source: &'static str,
    domain: &'static str,
    symbol: String,
    ts: u64,
    payload: String,
}

fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(Duration::from_millis(XADD_MAX_BACKOFF_MS))
}

pub fn spawn_redis_sink(
    bus: EventBus,
    redis_url: String,
    stream_prefix: String,
    dead_letter_path: String,
    metrics: Arc<AppMetrics>,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = bus.subscribe_events();
        let (local_tx, mut local_rx) = mpsc::channel::<Arc<DataEvent>>(REDIS_LOCAL_BUFFER);
        let drain_shutdown = shutdown.clone();
        let drain_handle = tokio::spawn(async move {
            loop {
                let received = tokio::select! {
                    _ = drain_shutdown.cancelled() => break,
                    received = rx.recv() => received,
                };
                match received {
                    Ok(event) => {
                        if local_tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "redis sink broadcast drain lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let client = match redis::Client::open(redis_url.as_str()) {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "redis client open failed");
                return;
            }
        };

        let mut conn = loop {
            if shutdown.is_cancelled() {
                return;
            }
            match client.get_multiplexed_tokio_connection().await {
                Ok(c) => {
                    info!("redis sink connected");
                    break c;
                }
                Err(e) => {
                    warn!(error = %e, "redis connect failed, retrying");
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }
                }
            }
        };

        let mut batch = Vec::with_capacity(XADD_BATCH_MAX);
        let mut flush_interval = tokio::time::interval(Duration::from_millis(XADD_BATCH_FLUSH_MS));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = flush_interval.tick(), if !batch.is_empty() => {
                    if !flush_redis_batch(
                        &client,
                        &mut conn,
                        &mut batch,
                        &dead_letter_path,
                        &metrics,
                    )
                    .await
                    {
                        return;
                    }
                    continue;
                }
                received = local_rx.recv() => received,
            };
            match received {
                Some(event) => {
                    batch.push(redis_event_row(&stream_prefix, event.as_ref()));
                    if batch.len() >= XADD_BATCH_MAX
                        && !flush_redis_batch(
                            &client,
                            &mut conn,
                            &mut batch,
                            &dead_letter_path,
                            &metrics,
                        )
                        .await
                    {
                        return;
                    }
                }
                None => break,
            }
        }

        if !batch.is_empty() {
            let _ = flush_redis_batch(&client, &mut conn, &mut batch, &dead_letter_path, &metrics)
                .await;
        }
        drain_handle.abort();
    })
}

fn redis_event_row(prefix: &str, event: &DataEvent) -> RedisEventRow {
    let (source, domain, symbol, ts) = redis_event_fields(event);
    let payload = match serde_json::to_string(event) {
        Ok(payload) => payload,
        Err(error) => {
            error!(%error, domain, source, "redis payload serialization failed");
            serde_json::json!({
                "type": "serialization_error",
                "source": source,
                "domain": domain,
                "symbol": symbol.as_deref(),
                "ts": ts,
                "error": error.to_string(),
            })
            .to_string()
        }
    };
    RedisEventRow {
        stream: redis_stream_name(prefix, event),
        source,
        domain,
        symbol: symbol.unwrap_or_else(|| "*".to_string()),
        ts,
        payload,
    }
}

async fn flush_redis_batch(
    client: &redis::Client,
    conn: &mut redis::aio::MultiplexedConnection,
    batch: &mut Vec<RedisEventRow>,
    dead_letter_path: &str,
    metrics: &AppMetrics,
) -> bool {
    if batch.is_empty() {
        return true;
    }

    let mut backoff = Duration::from_millis(XADD_INITIAL_BACKOFF_MS);
    let mut last_error = None;
    for attempt in 1..=XADD_MAX_ATTEMPTS {
        let mut pipe = redis::pipe();
        for row in batch.iter() {
            pipe.cmd("XADD")
                .arg(&row.stream)
                .arg("*")
                .arg("source")
                .arg(row.source)
                .arg("domain")
                .arg(row.domain)
                .arg("symbol")
                .arg(&row.symbol)
                .arg("ts")
                .arg(row.ts as i64)
                .arg("payload")
                .arg(&row.payload)
                .ignore();
        }

        let res: redis::RedisResult<()> = pipe.query_async(conn).await;
        match res {
            Ok(()) => {
                metrics.redis_xadd_total.inc_by(batch.len() as u64);
                batch.clear();
                return true;
            }
            Err(e) => {
                last_error = Some(e.to_string());
                warn!(
                    error=%e,
                    batch_len=batch.len(),
                    attempt,
                    "redis xadd pipeline failed, reconnecting"
                );
                match client.get_multiplexed_tokio_connection().await {
                    Ok(c) => *conn = c,
                    Err(e2) => warn!(error=%e2, "redis reconnect failed"),
                }
                tokio::time::sleep(backoff).await;
                backoff = next_backoff(backoff);
            }
        }
    }

    metrics.redis_dead_letter_total.inc_by(batch.len() as u64);
    let last_error = last_error.unwrap_or_else(|| "unknown".to_string());
    if let Err(error) = write_dead_letter_file(batch, &last_error, dead_letter_path).await {
        error!(
            error = %error,
            path = dead_letter_path,
            "redis dead-letter file write failed"
        );
    }
    error!(
        batch_len = batch.len(),
        attempts = XADD_MAX_ATTEMPTS,
        last_error = last_error,
        "redis xadd moved batch to dead letter after retries"
    );
    batch.clear();
    true
}

async fn write_dead_letter_file(
    batch: &[RedisEventRow],
    error: &str,
    path: &str,
) -> std::io::Result<()> {
    let rows = batch.to_vec();
    let error = error.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || write_dead_letter_file_blocking(&rows, &error, &path))
        .await
        .map_err(std::io::Error::other)?
}

fn write_dead_letter_file_blocking(
    batch: &[RedisEventRow],
    error: &str,
    path: &str,
) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for row in batch {
        let record = serde_json::json!({
            "reason": "redis_xadd_failed",
            "error": error,
            "row": row,
        });
        file.write_all(record.to_string().as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()
}

fn redis_stream_name(prefix: &str, event: &DataEvent) -> String {
    let (source, domain, symbol, _) = redis_event_fields(event);
    match symbol {
        Some(symbol) => format!("{prefix}:{domain}:{source}:{}", symbol.to_ascii_uppercase()),
        None => format!("{prefix}:{domain}:{source}"),
    }
}

fn redis_event_fields(event: &DataEvent) -> (&'static str, &'static str, Option<String>, u64) {
    match event {
        DataEvent::Tick(t) => (
            t.exchange,
            market_domain(t.market),
            Some(t.symbol.to_string()),
            t.ts_ms,
        ),
        DataEvent::FundingRate(t) => (t.exchange, "funding", Some(t.symbol.to_string()), t.ts_ms),
        DataEvent::OpenInterest(t) => (
            t.exchange,
            "open_interest",
            Some(t.symbol.to_string()),
            t.ts_ms,
        ),
        DataEvent::Trade(t) => (t.exchange, "trade", Some(t.symbol.to_string()), t.ts_ms),
        DataEvent::Liquidation(t) => (
            t.exchange,
            "liquidation",
            Some(t.symbol.to_string()),
            t.ts_ms,
        ),
        DataEvent::OrderBook(t) => (
            t.exchange,
            "order_book",
            Some(t.symbol.to_string()),
            t.ts_ms,
        ),
        DataEvent::ExternalSignal(t) => (
            t.source,
            "external_signal",
            t.symbol.as_deref().map(ToString::to_string),
            t.ts_ms,
        ),
        DataEvent::Heartbeat { exchange, ts_ms } => (*exchange, "heartbeat", None, *ts_ms),
    }
}

fn market_domain(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "quote_spot",
        MarketKind::Perp => "quote_perp",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RedisEventRow, XADD_MAX_BACKOFF_MS, next_backoff, redis_event_row, redis_stream_name,
        write_dead_letter_file,
    };
    use std::time::Duration;

    use crate::types::{DataEvent, FundingRateTick};

    #[test]
    fn retry_backoff_is_capped() {
        let capped = next_backoff(Duration::from_millis(XADD_MAX_BACKOFF_MS));
        assert_eq!(capped, Duration::from_millis(XADD_MAX_BACKOFF_MS));
    }

    #[test]
    fn redis_stream_names_include_event_domain() {
        let event = DataEvent::FundingRate(FundingRateTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            funding_rate: 0.01,
            next_funding_time_ms: None,
            mark_price: None,
            index_price: None,
            ts_ms: 1,
        });
        assert_eq!(
            redis_stream_name("ticks", &event),
            "ticks:funding:binance:BTCUSDT"
        );
    }

    #[test]
    fn redis_event_rows_are_ready_for_batch_xadd() {
        let event = DataEvent::FundingRate(FundingRateTick {
            exchange: "binance",
            symbol: "btcusdt".into(),
            funding_rate: 0.01,
            next_funding_time_ms: None,
            mark_price: None,
            index_price: None,
            ts_ms: 1,
        });

        let row = redis_event_row("ticks", &event);
        assert_eq!(
            row,
            RedisEventRow {
                stream: "ticks:funding:binance:BTCUSDT".into(),
                source: "binance",
                domain: "funding",
                symbol: "btcusdt".into(),
                ts: 1,
                payload: serde_json::to_string(&event).expect("test event serializes"),
            }
        );
    }

    #[tokio::test]
    async fn redis_dead_letters_are_written_to_jsonl() {
        let path = "data/test_redis_dead_letters.jsonl";
        let _ = std::fs::remove_file(path);
        let row = RedisEventRow {
            stream: "ticks:funding:binance:BTCUSDT".into(),
            source: "binance",
            domain: "funding",
            symbol: "BTCUSDT".into(),
            ts: 1,
            payload: "{}".into(),
        };

        write_dead_letter_file(&[row], "boom", path)
            .await
            .expect("dead-letter file should be writable in test workspace");

        let content = std::fs::read_to_string(path).expect("dead-letter file should exist");
        assert!(content.contains("\"reason\":\"redis_xadd_failed\""));
        assert!(content.contains("\"error\":\"boom\""));
        let _ = std::fs::remove_file(path);
    }
}
