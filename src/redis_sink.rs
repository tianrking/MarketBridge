use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{info, warn};

use crate::event_bus::{EventBus, EventSubscription, SharedEvent};
use crate::metrics::AppMetrics;
use crate::types::{DataEvent, MarketKind};

const XADD_MAX_ATTEMPTS: usize = 8;
const XADD_INITIAL_BACKOFF_MS: u64 = 100;
const XADD_MAX_BACKOFF_MS: u64 = 5_000;
const XADD_BATCH_MAX: usize = 100;
const XADD_BATCH_FLUSH_MS: u64 = 50;
const REDIS_LOCAL_BUFFER: usize = 100_000;

#[derive(Clone, Copy)]
struct RetryPolicy {
    attempts: usize,
    operation_timeout: Duration,
    initial_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: XADD_MAX_ATTEMPTS,
            operation_timeout: Duration::from_secs(3),
            initial_backoff: Duration::from_millis(XADD_INITIAL_BACKOFF_MS),
        }
    }
}
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
) -> JoinHandle<anyhow::Result<()>> {
    // Subscribe before returning so events published immediately after startup
    // are subject to the same explicit loss policy as later events.
    let rx = bus.subscribe_events();
    tokio::spawn(async move {
        let client = redis::Client::open(redis_url.as_str()).context("redis client open failed")?;
        let (local_tx, mut local_rx) = mpsc::channel(REDIS_LOCAL_BUFFER);
        let intake_stop = shutdown.child_token();
        let drain = AbortOnDropHandle::new(tokio::spawn(drain_events(
            rx,
            local_tx,
            metrics.clone(),
            intake_stop.clone(),
        )));
        let result = write_events(
            &client,
            &mut local_rx,
            &stream_prefix,
            &dead_letter_path,
            &metrics,
            &shutdown,
            RetryPolicy::default(),
        )
        .await;
        // All exits, including initialization/write failures and task abortion,
        // own the intake task. Never detach a hidden consumer after sink failure.
        intake_stop.cancel();
        local_rx.close();
        let drain_result = drain
            .await
            .context("redis intake task failed")
            .and_then(|result| result);
        let outcome = result.and(drain_result);
        info!(
            accepted = metrics.redis_accepted_total.get(),
            acknowledged = metrics.redis_xadd_total.get(),
            dead_letter_synced = metrics.redis_dead_letter_total.get(),
            pending = metrics.redis_pending_events.get(),
            lagged = metrics.redis_lagged_total.get(),
            success = outcome.is_ok(),
            "redis sink stopped; local acceptance accounting"
        );
        outcome
    })
}

async fn drain_events(
    mut rx: EventSubscription,
    local_tx: mpsc::Sender<Arc<SharedEvent>>,
    metrics: Arc<AppMetrics>,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        // Reserve first: cancellation never drops an event already removed
        // from the subscription while waiting for room in the local queue.
        let permit = tokio::select! {
            biased;
            _ = stop.cancelled() => return Ok(()),
            permit = local_tx.reserve() => match permit {
                Ok(permit) => permit,
                Err(_) => return Ok(()),
            },
        };
        let received = tokio::select! {
            biased;
            _ = stop.cancelled() => return Ok(()),
            received = rx.recv() => received,
        };
        match received {
            Ok(event) => {
                metrics.redis_pending_events.inc();
                metrics.redis_accepted_total.inc();
                permit.send(event);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                metrics.redis_lagged_total.inc_by(skipped);
                anyhow::bail!("redis subscriber lost {skipped} bus events; archive is incomplete");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

async fn write_events(
    client: &redis::Client,
    local_rx: &mut mpsc::Receiver<Arc<SharedEvent>>,
    stream_prefix: &str,
    dead_letter_path: &str,
    metrics: &AppMetrics,
    shutdown: &CancellationToken,
    policy: RetryPolicy,
) -> anyhow::Result<()> {
    let mut conn = None;
    let mut batch = Vec::with_capacity(XADD_BATCH_MAX);
    let mut flush_interval = tokio::time::interval(Duration::from_millis(XADD_BATCH_FLUSH_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut draining = false;
    loop {
        let received = tokio::select! {
            biased;
            _ = shutdown.cancelled(), if !draining => {
                // Freeze the local acceptance boundary and drain every event
                // already accepted. Intake releases its outstanding permit on
                // the same cancellation token, allowing recv() to reach None.
                local_rx.close();
                draining = true;
                continue;
            }
            _ = flush_interval.tick(), if !batch.is_empty() && !draining => {
                flush_redis_batch(client, &mut conn, &mut batch, dead_letter_path,
                    metrics, shutdown, policy).await?;
                continue;
            }
            received = local_rx.recv() => received,
        };
        match received {
            Some(event) => {
                batch.push(redis_event_row(stream_prefix, event.as_ref()));
                if batch.len() >= XADD_BATCH_MAX {
                    flush_redis_batch(
                        client,
                        &mut conn,
                        &mut batch,
                        dead_letter_path,
                        metrics,
                        shutdown,
                        policy,
                    )
                    .await?;
                }
            }
            None => break,
        }
    }
    flush_redis_batch(
        client,
        &mut conn,
        &mut batch,
        dead_letter_path,
        metrics,
        shutdown,
        policy,
    )
    .await
}

fn redis_event_row(prefix: &str, shared: &SharedEvent) -> RedisEventRow {
    let event = shared.event.as_ref();
    let (source, domain, symbol, ts) = redis_event_fields(event);
    RedisEventRow {
        stream: redis_stream_name(prefix, event),
        source,
        domain,
        symbol: symbol.unwrap_or_else(|| "*".to_string()),
        ts,
        payload: shared.json().as_ref().to_string(),
    }
}

async fn flush_redis_batch(
    client: &redis::Client,
    conn: &mut Option<redis::aio::MultiplexedConnection>,
    batch: &mut Vec<RedisEventRow>,
    dead_letter_path: &str,
    metrics: &AppMetrics,
    shutdown: &CancellationToken,
    policy: RetryPolicy,
) -> anyhow::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut backoff = policy.initial_backoff;
    let mut last_error = "shutdown before Redis acknowledgement".to_string();
    for attempt in 1..=policy.attempts {
        // A cancelled or timed-out pipeline may already have applied some
        // XADDs. Preserve the whole unacknowledged batch, allowing duplicates.
        let result = tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            result = tokio::time::timeout(policy.operation_timeout, async {
                if conn.is_none() {
                    *conn = Some(client.get_multiplexed_tokio_connection().await?);
                    info!("redis sink connected");
                }
                let mut pipe = redis::pipe();
                for row in batch.iter() {
                    pipe.cmd("XADD").arg(&row.stream).arg("*")
                        .arg("source").arg(row.source)
                        .arg("domain").arg(row.domain)
                        .arg("symbol").arg(&row.symbol)
                        .arg("ts").arg(row.ts)
                        .arg("payload").arg(&row.payload).ignore();
                }
                let connection = conn.as_mut().expect("connection established above");
                pipe.query_async::<()>(connection).await
            }) => result,
        };
        match result {
            Ok(Ok(())) => {
                metrics.redis_xadd_total.inc_by(batch.len() as u64);
                metrics.redis_pending_events.sub(batch.len() as i64);
                batch.clear();
                return Ok(());
            }
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => last_error = "Redis connection or pipeline acknowledgement timed out".into(),
        }
        // Never reuse a connection with an ambiguous/unfinished response.
        *conn = None;
        warn!(error = %last_error, batch_len = batch.len(), attempt, "redis batch attempt failed");
        if attempt < policy.attempts {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(backoff) => {}
            }
            backoff = next_backoff(backoff);
        }
    }
    // Shutdown bypasses network retries; queued events go to the same durable
    // recovery file. Success is counted only after write + flush + sync_all.
    persist_dead_letter(batch, &last_error, dead_letter_path, metrics).await
}

async fn persist_dead_letter(
    batch: &mut Vec<RedisEventRow>,
    error: &str,
    path: &str,
    metrics: &AppMetrics,
) -> anyhow::Result<()> {
    if let Err(failure) = write_dead_letter_file(batch, error, path).await {
        metrics.redis_dead_letter_failures_total.inc();
        return Err(failure).with_context(|| format!(
            "redis dead-letter persistence failed for {} events at {path}; batch unacknowledged", batch.len()
        ));
    }
    metrics.redis_dead_letter_total.inc_by(batch.len() as u64);
    metrics.redis_pending_events.sub(batch.len() as i64);
    warn!(
        batch_len = batch.len(),
        path, "redis batch synced to dead letter; recovery required"
    );
    batch.clear();
    Ok(())
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
    file.flush()?;
    file.sync_all()
}

// One physical stream per durable origin avoids cross-writer sequence ID
// collisions. Existing IDs must have identical content; trimmed/missing old
// IDs fail closed rather than silently treating them as delivered.
const IDEMPOTENT_XADD: &str = r#"
local old = redis.call('XRANGE', KEYS[1], ARGV[1], ARGV[1])
if #old > 0 then
  local identity = nil
  local payload = nil
  local crc = nil
  for i = 1, #old[1][2], 2 do
    if old[1][2][i] == 'event_id' then identity = old[1][2][i+1] end
    if old[1][2][i] == 'payload' then payload = old[1][2][i+1] end
    if old[1][2][i] == 'event_crc32' then crc = old[1][2][i+1] end
  end
  if identity ~= ARGV[2] or payload ~= ARGV[3] or crc ~= ARGV[8] then
    return redis.error_reply('EVENT_ID_COLLISION')
  end
  return old[1][1]
end
return redis.call('XADD', KEYS[1], ARGV[1], 'event_id', ARGV[2],
  'payload', ARGV[3], 'source', ARGV[4], 'domain', ARGV[5],
  'symbol', ARGV[6], 'ts', ARGV[7], 'event_crc32', ARGV[8], 'received_at_ms', ARGV[9])
"#;

pub fn spawn_outbox_sink(
    outbox: crate::outbox::Outbox,
    redis_url: String,
    require_aof: bool,
    metrics: Arc<AppMetrics>,
    shutdown: CancellationToken,
) -> JoinHandle<anyhow::Result<()>> {
    outbox.configure_delivery(require_aof);
    tokio::spawn(async move {
        let client =
            redis::Client::open(redis_url.as_str()).context("durable Redis client open failed")?;
        let mut conn = None;
        let mut backoff = Duration::from_millis(100);
        loop {
            if shutdown.is_cancelled() {
                break;
            }
            // AOF everysec amortizes one fsync barrier across this bounded batch.
            // The legacy 100-row batch would cap durable throughput near 100/s.
            let rows = outbox.pending(4096).await?;
            let status = outbox.status().await?;
            metrics
                .redis_pending_events
                .set(status.pending_records as i64);
            let batch_started = std::time::Instant::now();
            let result = tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                result = tokio::time::timeout(Duration::from_secs(3), async {
                    if conn.is_none() { conn = Some(client.get_multiplexed_tokio_connection().await?); }
                    let conn = conn.as_mut().expect("durable connection initialized");
                    if rows.is_empty() && status.delivery.verified {
                        redis::cmd("PING").query_async::<String>(conn).await?;
                        Ok(())
                    } else { deliver_outbox_batch(conn,&rows,outbox.origin(),require_aof).await }
                }) => result,
            };
            metrics
                .redis_batch_seconds
                .observe(batch_started.elapsed().as_secs_f64());
            match result {
                Ok(Ok(())) => {
                    outbox
                        .acknowledge(rows.iter().map(|r| r.identity.clone()).collect())
                        .await?;
                    metrics.redis_xadd_total.inc_by(rows.len() as u64);
                    outbox.delivery_result(None);
                    backoff = Duration::from_millis(100);
                    if rows.is_empty() {
                        tokio::select! { _=shutdown.cancelled()=>break, _=tokio::time::sleep(Duration::from_secs(1))=>{}, _=outbox.changed()=>{} }
                    }
                }
                failure => {
                    conn = None;
                    let error = match failure {
                        Ok(Err(error)) => error.to_string(),
                        Err(_) => "durable Redis delivery/verification timed out".into(),
                        _ => unreachable!(),
                    };
                    outbox.delivery_result(Some(&error));
                    metrics.redis_delivery_failures_total.inc();
                    warn!(%error,pending=status.pending_records,"durable Redis delivery failed; rows retained in outbox");
                    tokio::select! { _=shutdown.cancelled()=>break, _=tokio::time::sleep(backoff)=>{} }
                    backoff = next_backoff(backoff);
                }
            }
        }
        let status = outbox.status().await?;
        metrics
            .redis_pending_events
            .set(status.pending_records as i64);
        info!(
            pending = status.pending_records,
            bytes = status.pending_bytes,
            "durable Redis sink stopped; unacknowledged events remain in SQLite"
        );
        Ok(())
    })
}

async fn deliver_outbox_batch(
    conn: &mut redis::aio::MultiplexedConnection,
    rows: &[crate::outbox::PendingEvent],
    origin: &str,
    require_aof: bool,
) -> anyhow::Result<()> {
    let mut pipe = redis::pipe();
    for row in rows {
        let value: serde_json::Value = serde_json::from_str(&row.payload)?;
        let source = value["exchange"]
            .as_str()
            .or_else(|| value["source"].as_str())
            .unwrap_or("unknown");
        let domain = match value["type"].as_str().unwrap_or("unknown") {
            "tick" => {
                if value["market"] == "spot" {
                    "quote_spot"
                } else {
                    "quote_perp"
                }
            }
            "funding_rate" => "funding",
            kind => kind,
        };
        let symbol = value["symbol"].as_str().unwrap_or("*");
        let timestamp = value["ts_ms"].as_u64().unwrap_or(0);
        pipe.cmd("EVAL")
            .arg(IDEMPOTENT_XADD)
            .arg(1)
            .arg(row.physical_stream())
            .arg(format!("{}-0", row.identity.sequence))
            .arg(row.identity.id())
            .arg(&row.payload)
            .arg(source)
            .arg(domain)
            .arg(symbol)
            .arg(timestamp)
            .arg(row.crc32)
            .arg(row.received_at_ms)
            .ignore();
    }
    if !rows.is_empty() {
        pipe.query_async::<()>(conn).await?;
    }
    // Force a write on this connection even when every EVAL was a duplicate.
    // WAITAOF then covers the global AOF offset after the original XADDs, not
    // merely a fresh connection's empty write history after an ACK was lost.
    redis::cmd("SET")
        .arg(format!("marketbridge:outbox:barrier:{origin}"))
        .arg(rows.last().map_or(0, |r| r.identity.sequence))
        .query_async::<()>(conn)
        .await?;
    if require_aof {
        let (local, _replicas): (u64, u64) = redis::cmd("WAITAOF")
            .arg(1)
            .arg(0)
            .arg(1000)
            .query_async(conn)
            .await?;
        anyhow::ensure!(
            local >= 1,
            "Redis local AOF fsync not confirmed; outbox retained"
        );
    }
    Ok(())
}

pub(crate) fn redis_stream_name(prefix: &str, event: &DataEvent) -> String {
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

    use std::sync::Arc;

    use crate::event_bus::SharedEvent;
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

        let shared = SharedEvent::new(Arc::new(event.clone()));
        let row = redis_event_row("ticks", &shared);
        assert_eq!(
            row,
            RedisEventRow {
                stream: "ticks:funding:binance:BTCUSDT".into(),
                source: "binance",
                domain: "funding",
                symbol: "btcusdt".into(),
                ts: 1,
                payload: shared.json().as_ref().to_owned(),
            }
        );
    }

    #[tokio::test]
    async fn redis_dead_letters_are_written_to_jsonl() {
        let root = std::env::temp_dir().join(format!(
            "marketbridge-redis-test-{}-{}",
            std::process::id(),
            crate::types::now_ms()
        ));
        let path = root.join("dead_letters.jsonl");
        let row = RedisEventRow {
            stream: "ticks:funding:binance:BTCUSDT".into(),
            source: "binance",
            domain: "funding",
            symbol: "BTCUSDT".into(),
            ts: 1,
            payload: "{}".into(),
        };

        write_dead_letter_file(&[row], "boom", path.to_string_lossy().as_ref())
            .await
            .expect("dead-letter file should be writable in test workspace");

        let content = std::fs::read_to_string(&path).expect("dead-letter file should exist");
        assert!(content.contains("\"reason\":\"redis_xadd_failed\""));
        assert!(content.contains("\"error\":\"boom\""));
        let _ = std::fs::remove_dir_all(root);
    }

    // A local RESP peer exercises the actual Redis client, pipelines and
    // reconnects without requiring an external Redis service in platform CI.
    #[derive(Clone, Copy)]
    enum ResponseMode {
        Ack,
        Reject,
        Stall,
        LoseFirstAck,
    }

    struct RedisPeer {
        url: String,
        xadds: Arc<std::sync::atomic::AtomicUsize>,
        _task: tokio_util::task::AbortOnDropHandle<()>,
    }

    impl RedisPeer {
        async fn start(mode: ResponseMode) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("redis://{}", listener.local_addr().unwrap());
            let xadds = Arc::new(AtomicUsize::new(0));
            let seen = xadds.clone();
            let task = tokio::spawn(async move {
                let mut clients = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        accepted = listener.accept() => {
                            let (socket, _) = accepted.unwrap();
                            let seen = seen.clone();
                            clients.spawn(async move {
                                let (read, mut write) = socket.into_split();
                                let mut read = BufReader::new(read);
                                loop {
                                    let mut line = String::new();
                                    if read.read_line(&mut line).await.unwrap_or(0) == 0 { break; }
                                    let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                                    let mut args = Vec::new();
                                    for _ in 0..count {
                                        line.clear();
                                        read.read_line(&mut line).await.unwrap();
                                        let size: usize = line.trim().strip_prefix('$').unwrap().parse().unwrap();
                                        let mut value = vec![0; size + 2];
                                        read.read_exact(&mut value).await.unwrap();
                                        value.truncate(size);
                                        args.push(value);
                                    }
                                    let response: &[u8] = if args[0].eq_ignore_ascii_case(b"XADD") {
                                        let index = seen.fetch_add(1, Ordering::SeqCst);
                                        match mode {
                                            ResponseMode::Stall => std::future::pending().await,
                                            ResponseMode::LoseFirstAck if index == 0 => break,
                                            ResponseMode::Reject => b"-ERR injected write failure\r\n",
                                            _ => b"$3\r\n1-0\r\n",
                                        }
                                    } else { b"+OK\r\n" };
                                    if write.write_all(response).await.is_err() { break; }
                                }
                            });
                        }
                        result = clients.join_next(), if !clients.is_empty() => {
                            result.unwrap().unwrap();
                        }
                    }
                }
            });
            Self {
                url,
                xadds,
                _task: tokio_util::task::AbortOnDropHandle::new(task),
            }
        }
    }

    fn test_event(ts_ms: u64) -> Arc<SharedEvent> {
        Arc::new(SharedEvent::new(Arc::new(DataEvent::FundingRate(
            FundingRateTick {
                exchange: "binance",
                symbol: "BTCUSDT".into(),
                funding_rate: 0.01,
                next_funding_time_ms: None,
                mark_price: None,
                index_price: None,
                ts_ms,
            },
        ))))
    }

    fn test_path(label: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "marketbridge-redis-{label}-{}-{}-{}",
            std::process::id(),
            crate::types::now_ms(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    async fn until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("fixture condition reached");
    }

    #[tokio::test]
    async fn dead_letter_failure_preserves_batch_and_does_not_claim_success() {
        let root = test_path("blocked");
        std::fs::create_dir_all(&root).unwrap();
        // Opening a directory as an append-only file fails on every platform,
        // including root in Linux CI, unlike a permission-bit-only fixture.
        let mut batch = vec![super::redis_event_row("ticks", &test_event(1))];
        let before = batch.clone();
        let metrics = crate::metrics::AppMetrics::new();
        metrics.redis_pending_events.inc();
        let error =
            super::persist_dead_letter(&mut batch, "fixture", root.to_str().unwrap(), &metrics)
                .await
                .unwrap_err();
        assert!(error.to_string().contains("unacknowledged"));
        assert_eq!(batch, before);
        assert_eq!(metrics.redis_dead_letter_total.get(), 0);
        assert_eq!(metrics.redis_dead_letter_failures_total.get(), 1);
        assert_eq!(metrics.redis_pending_events.get(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn shutdown_drains_all_accepted_events_when_redis_stalls() {
        let peer = RedisPeer::start(ResponseMode::Stall).await;
        let bus = crate::event_bus::EventBus::new_sharded(512, 10_000, 1);
        let metrics = crate::metrics::AppMetrics::new();
        let stop = tokio_util::sync::CancellationToken::new();
        let path = test_path("shutdown").join("dead.jsonl");
        let task = super::spawn_redis_sink(
            bus.clone(),
            peer.url.clone(),
            "ticks".into(),
            path.to_str().unwrap().into(),
            metrics.clone(),
            stop.clone(),
        );
        // More than two full batches, with a Redis acknowledgement stuck.
        for ts in 0..237 {
            bus.publish_shared_event(test_event(ts).event.clone());
        }
        until(|| metrics.redis_accepted_total.get() == 237).await;
        until(|| peer.xadds.load(std::sync::atomic::Ordering::SeqCst) > 0).await;
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let records: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let timestamps: std::collections::HashSet<_> = records
            .iter()
            .map(|row| row["row"]["ts"].as_u64().unwrap())
            .collect();
        assert_eq!(records.len(), 237);
        assert_eq!(timestamps.len(), 237);
        assert_eq!(metrics.redis_dead_letter_total.get(), 237);
        assert_eq!(metrics.redis_pending_events.get(), 0);
        assert_eq!(metrics.redis_xadd_total.get(), 0);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn failed_shutdown_persistence_returns_service_error() {
        let peer = RedisPeer::start(ResponseMode::Stall).await;
        let bus = crate::event_bus::EventBus::new_sharded(16, 10_000, 1);
        let metrics = crate::metrics::AppMetrics::new();
        let stop = tokio_util::sync::CancellationToken::new();
        let path = test_path("shutdown-failure");
        std::fs::create_dir_all(&path).unwrap();
        let task = super::spawn_redis_sink(
            bus.clone(),
            peer.url.clone(),
            "ticks".into(),
            path.to_str().unwrap().into(),
            metrics.clone(),
            stop.clone(),
        );
        bus.publish_shared_event(test_event(1).event.clone());
        until(|| metrics.redis_accepted_total.get() == 1).await;
        stop.cancel();
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("dead-letter persistence failed")
        );
        assert_eq!(metrics.redis_dead_letter_total.get(), 0);
        assert_eq!(metrics.redis_pending_events.get(), 1);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn redis_timeout_and_rejection_both_preserve_rows_in_dead_letter() {
        for mode in [ResponseMode::Stall, ResponseMode::Reject] {
            let peer = RedisPeer::start(mode).await;
            let client = redis::Client::open(peer.url.as_str()).unwrap();
            let path = test_path("retry").join("dead.jsonl");
            let metrics = crate::metrics::AppMetrics::new();
            metrics.redis_pending_events.inc();
            let mut batch = vec![super::redis_event_row("ticks", &test_event(1))];
            let policy = super::RetryPolicy {
                attempts: 2,
                operation_timeout: Duration::from_millis(100),
                initial_backoff: Duration::from_millis(1),
            };
            tokio::time::timeout(
                Duration::from_secs(3),
                super::flush_redis_batch(
                    &client,
                    &mut None,
                    &mut batch,
                    path.to_str().unwrap(),
                    &metrics,
                    &tokio_util::sync::CancellationToken::new(),
                    policy,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(batch.is_empty());
            assert_eq!(metrics.redis_dead_letter_total.get(), 1);
            assert_eq!(metrics.redis_pending_events.get(), 0);
            assert_eq!(metrics.redis_xadd_total.get(), 0);
            let record: serde_json::Value =
                serde_json::from_str(std::fs::read_to_string(&path).unwrap().trim()).unwrap();
            assert_eq!(record["row"]["ts"], 1);
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }

    #[tokio::test]
    async fn lost_ack_retries_without_claiming_exactly_once() {
        let peer = RedisPeer::start(ResponseMode::LoseFirstAck).await;
        let client = redis::Client::open(peer.url.as_str()).unwrap();
        let metrics = crate::metrics::AppMetrics::new();
        metrics.redis_pending_events.inc();
        let mut batch = vec![super::redis_event_row("ticks", &test_event(1))];
        let path = test_path("lost-ack");
        super::flush_redis_batch(
            &client,
            &mut None,
            &mut batch,
            path.to_str().unwrap(),
            &metrics,
            &tokio_util::sync::CancellationToken::new(),
            super::RetryPolicy {
                attempts: 2,
                operation_timeout: Duration::from_secs(1),
                initial_backoff: Duration::from_millis(1),
            },
        )
        .await
        .unwrap();
        assert_eq!(peer.xadds.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(metrics.redis_xadd_total.get(), 1);
        assert_eq!(metrics.redis_dead_letter_total.get(), 0);
        assert_eq!(metrics.redis_pending_events.get(), 0);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn acknowledged_pipeline_needs_no_dead_letter() {
        let peer = RedisPeer::start(ResponseMode::Ack).await;
        let client = redis::Client::open(peer.url.as_str()).unwrap();
        let metrics = crate::metrics::AppMetrics::new();
        metrics.redis_pending_events.set(2);
        let mut batch = vec![
            super::redis_event_row("ticks", &test_event(1)),
            super::redis_event_row("ticks", &test_event(2)),
        ];
        let path = test_path("ack");
        super::flush_redis_batch(
            &client,
            &mut None,
            &mut batch,
            path.to_str().unwrap(),
            &metrics,
            &tokio_util::sync::CancellationToken::new(),
            super::RetryPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(peer.xadds.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(metrics.redis_xadd_total.get(), 2);
        assert_eq!(metrics.redis_dead_letter_total.get(), 0);
        assert_eq!(metrics.redis_pending_events.get(), 0);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn subscriber_loss_is_counted_and_returns_an_error() {
        let bus = crate::event_bus::EventBus::new_sharded(2, 10_000, 1);
        let subscription = bus.subscribe_events();
        // This test runs on a current-thread runtime. No forwarder can run
        // before the ring has deterministically overflowed.
        for ts in 0..16 {
            bus.publish_shared_event(test_event(ts).event.clone());
        }
        let metrics = crate::metrics::AppMetrics::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let result = super::drain_events(
            subscription,
            tx,
            metrics.clone(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("archive is incomplete")
        );
        assert_eq!(metrics.redis_lagged_total.get(), 14);
        assert_eq!(metrics.redis_accepted_total.get(), 0);
    }

    #[tokio::test]
    async fn full_local_queue_does_not_prevent_intake_shutdown() {
        let bus = crate::event_bus::EventBus::new_sharded(16, 10_000, 1);
        let subscription = bus.subscribe_events();
        let metrics = crate::metrics::AppMetrics::new();
        let stop = tokio_util::sync::CancellationToken::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(super::drain_events(
            subscription,
            tx,
            metrics.clone(),
            stop.clone(),
        ));
        for ts in 0..4 {
            bus.publish_shared_event(test_event(ts).event.clone());
        }
        until(|| metrics.redis_accepted_total.get() == 1).await;
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(metrics.redis_accepted_total.get(), 1);
        assert!(rx.recv().await.is_some());
        assert!(rx.recv().await.is_none());
    }

    struct ActualRedis {
        root: std::path::PathBuf,
        child: std::process::Child,
        executable: std::path::PathBuf,
        port: u16,
        aof: bool,
    }
    impl ActualRedis {
        fn start(aof: bool) -> Self {
            let executable = std::env::var_os("MARKETBRIDGE_TEST_REDIS_SERVER")
                .map(std::path::PathBuf::from)
                .expect("set MARKETBRIDGE_TEST_REDIS_SERVER to Redis >=7.2 redis-server");
            let root = test_path("actual-redis");
            std::fs::create_dir_all(&root).unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            let child = Self::launch(&executable, &root, port, aof);
            Self {
                root,
                child,
                executable,
                port,
                aof,
            }
        }
        fn launch(
            executable: &std::path::Path,
            root: &std::path::Path,
            port: u16,
            aof: bool,
        ) -> std::process::Child {
            std::process::Command::new(executable)
                .arg("--bind")
                .arg("127.0.0.1")
                .arg("--port")
                .arg(port.to_string())
                .arg("--protected-mode")
                .arg("yes")
                .arg("--dir")
                .arg(root)
                .arg("--save")
                .arg("")
                .arg("--appendonly")
                .arg(if aof { "yes" } else { "no" })
                .arg("--appendfsync")
                .arg("always")
                .arg("--logfile")
                .arg(root.join("redis.log"))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap()
        }
        async fn connect(&mut self) -> redis::aio::MultiplexedConnection {
            let client = redis::Client::open(format!("redis://127.0.0.1:{}", self.port)).unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    assert!(
                        self.child.try_wait().unwrap().is_none(),
                        "actual Redis exited: {}",
                        std::fs::read_to_string(self.root.join("redis.log")).unwrap_or_default()
                    );
                    if let Ok(conn) = client.get_multiplexed_tokio_connection().await {
                        return conn;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap()
        }
        async fn restart(&mut self) -> redis::aio::MultiplexedConnection {
            self.child.kill().unwrap();
            self.child.wait().unwrap();
            self.child = Self::launch(&self.executable, &self.root, self.port, self.aof);
            self.connect().await
        }
    }
    impl Drop for ActualRedis {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    #[ignore = "requires local Redis >=7.2; run with MARKETBRIDGE_TEST_REDIS_SERVER and --ignored"]
    async fn durable_outbox_against_real_redis_survives_ack_loss_replay_and_restart() {
        let mut server = ActualRedis::start(true);
        let mut conn = server.connect().await;
        let root = test_path("durable");
        std::fs::create_dir_all(&root).unwrap();
        let box_ = crate::outbox::Outbox::open(
            &root.join("events.sqlite"),
            "ticks",
            1024 * 1024,
            4 * 1024 * 1024,
        )
        .unwrap();
        box_.append(vec![
            (test_event(1).event.clone(), 1),
            (test_event(2).event.clone(), 2),
        ])
        .await
        .unwrap();
        let export = root.join("pending.jsonl");
        box_.export(&export).await.unwrap();
        let rows = box_.pending(100).await.unwrap();
        let key = rows[0].physical_stream();
        // Write succeeds but local ACK deletion never happens, as after an
        // acknowledgement lost to a socket failure or a process crash.
        super::deliver_outbox_batch(&mut conn, &rows, box_.origin(), true)
            .await
            .unwrap();
        assert_eq!(box_.status().await.unwrap().pending_records, 2);
        drop(conn);
        let mut conn = server.restart().await;
        let count: u64 = redis::cmd("XLEN")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert_eq!(count, 2);
        for _ in 0..2 {
            let stop = tokio_util::sync::CancellationToken::new();
            let metrics = crate::metrics::AppMetrics::new();
            let task = super::spawn_outbox_sink(
                box_.clone(),
                format!("redis://127.0.0.1:{}", server.port),
                true,
                metrics.clone(),
                stop.clone(),
            );
            tokio::time::timeout(Duration::from_secs(5), async {
                while box_.status().await.unwrap().pending_records > 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            stop.cancel();
            task.await.unwrap().unwrap();
            let count: u64 = redis::cmd("XLEN")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap();
            assert_eq!(count, 2);
            assert_eq!(metrics.redis_xadd_total.get(), 2);
            box_.replay(&export).await.unwrap();
        }
        // Trimming away an old event destroys deduplication evidence. Do not
        // silently acknowledge it based only on the stream high watermark.
        redis::cmd("XDEL")
            .arg(&key)
            .arg(format!("{}-0", rows[0].identity.sequence))
            .query_async::<u64>(&mut conn)
            .await
            .unwrap();
        assert!(
            super::deliver_outbox_batch(&mut conn, &rows, box_.origin(), true)
                .await
                .is_err()
        );
        assert_eq!(box_.status().await.unwrap().pending_records, 2);
        drop(conn);
        drop(box_);
        std::fs::remove_dir_all(&root).unwrap();
        drop(server);

        let mut no_aof = ActualRedis::start(false);
        let mut conn = no_aof.connect().await;
        let root = test_path("durability-required");
        let box_ = crate::outbox::Outbox::open(
            &root.join("events.sqlite"),
            "ticks",
            1024 * 1024,
            4 * 1024 * 1024,
        )
        .unwrap();
        box_.append(vec![(test_event(3).event.clone(), 3)])
            .await
            .unwrap();
        let rows = box_.pending(10).await.unwrap();
        assert!(
            super::deliver_outbox_batch(&mut conn, &rows, box_.origin(), true)
                .await
                .is_err(),
            "plain Redis ACK must not substitute for required AOF confirmation"
        );
        assert_eq!(box_.status().await.unwrap().pending_records, 1);
        drop(conn);
        drop(box_);
        std::fs::remove_dir_all(root).unwrap();
    }
}
