use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;

const XADD_MAX_ATTEMPTS: usize = 8;
const XADD_INITIAL_BACKOFF_MS: u64 = 100;
const XADD_MAX_BACKOFF_MS: u64 = 5_000;

fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(Duration::from_millis(XADD_MAX_BACKOFF_MS))
}

pub fn spawn_redis_sink(
    bus: EventBus,
    redis_url: String,
    stream_prefix: String,
    metrics: Arc<AppMetrics>,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = bus.subscribe();

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

        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                received = rx.recv() => received,
            };
            match received {
                Ok(t) => {
                    let stream = format!("{}:{}", stream_prefix, t.symbol.to_ascii_uppercase());
                    let payload = serde_json::to_string(&t).unwrap_or_else(|_| "{}".to_string());
                    let mut wrote = false;
                    let mut backoff = Duration::from_millis(XADD_INITIAL_BACKOFF_MS);
                    let mut last_error = None;
                    for attempt in 1..=XADD_MAX_ATTEMPTS {
                        let res: redis::RedisResult<String> = redis::cmd("XADD")
                            .arg(&stream)
                            .arg("*")
                            .arg("exchange")
                            .arg(t.exchange)
                            .arg("market")
                            .arg(t.market)
                            .arg("symbol")
                            .arg(&t.symbol)
                            .arg("ts")
                            .arg(t.ts as i64)
                            .arg("payload")
                            .arg(&payload)
                            .query_async(&mut conn)
                            .await;
                        match res {
                            Ok(_) => {
                                metrics.redis_xadd_total.inc();
                                wrote = true;
                                break;
                            }
                            Err(e) => {
                                last_error = Some(e.to_string());
                                warn!(error=%e, stream=%stream, attempt, "redis xadd failed, reconnecting");
                                match client.get_multiplexed_tokio_connection().await {
                                    Ok(c) => conn = c,
                                    Err(e2) => {
                                        warn!(error=%e2, "redis reconnect failed");
                                    }
                                }
                                tokio::select! {
                                    _ = shutdown.cancelled() => return,
                                    _ = tokio::time::sleep(backoff) => {}
                                }
                                backoff = next_backoff(backoff);
                            }
                        }
                    }
                    if !wrote {
                        metrics.redis_dead_letter_total.inc();
                        error!(
                            stream=%stream,
                            attempts = XADD_MAX_ATTEMPTS,
                            last_error = last_error.as_deref().unwrap_or("unknown"),
                            "redis xadd moved event to dead letter after retries"
                        );
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "redis sink lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{XADD_MAX_BACKOFF_MS, next_backoff};
    use std::time::Duration;

    #[test]
    fn retry_backoff_is_capped() {
        let capped = next_backoff(Duration::from_millis(XADD_MAX_BACKOFF_MS));
        assert_eq!(capped, Duration::from_millis(XADD_MAX_BACKOFF_MS));
    }
}
