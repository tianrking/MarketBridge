use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;
use crate::types::{DataEvent, MarketKind};

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
        let mut rx = bus.subscribe_events();

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
                Ok(event) => {
                    let stream = redis_stream_name(&stream_prefix, &event);
                    let payload =
                        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                    let (source, domain, symbol, ts) = redis_event_fields(&event);
                    let mut wrote = false;
                    let mut backoff = Duration::from_millis(XADD_INITIAL_BACKOFF_MS);
                    let mut last_error = None;
                    for attempt in 1..=XADD_MAX_ATTEMPTS {
                        let res: redis::RedisResult<String> = redis::cmd("XADD")
                            .arg(&stream)
                            .arg("*")
                            .arg("source")
                            .arg(source)
                            .arg("domain")
                            .arg(domain)
                            .arg("symbol")
                            .arg(symbol.as_deref().unwrap_or("*"))
                            .arg("ts")
                            .arg(ts as i64)
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
    use super::{XADD_MAX_BACKOFF_MS, next_backoff, redis_stream_name};
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
}
