use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::time::interval;
use tracing::warn;

use crate::api::ApiState;
use crate::api::streaming::{
    EnvelopeFilter, SUPPORTED_STREAM_DOMAINS, StreamDomainFilter, TickFilter, event_matches,
    send_envelope, send_event, send_json, send_ws,
};
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;
use crate::event_bus::EventBus;
use crate::metrics::AppMetrics;

#[derive(Debug, Deserialize, Default)]
pub struct TickFilterQuery {
    pub symbols: Option<String>,
    pub exchanges: Option<String>,
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct V1StreamQuery {
    pub domains: Option<String>,
    pub symbols: Option<String>,
    pub exchanges: Option<String>,
    pub product_type: Option<String>,
    pub include_stale: Option<bool>,
    pub snapshot_interval_ms: Option<u64>,
}

pub async fn ws_ticks(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Query(q): Query<TickFilterQuery>,
) -> impl IntoResponse {
    let bus = state.bus.clone();
    let metrics = state.metrics.clone();
    ws.on_upgrade(move |socket| ws_loop(socket, bus, q, metrics))
}

pub async fn v1_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Query(q): Query<V1StreamQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| v1_stream_loop(socket, state, q))
}

async fn v1_stream_loop(mut socket: WebSocket, state: Arc<ApiState>, q: V1StreamQuery) {
    let domains = StreamDomainFilter::from_query(q.domains.clone());
    if !domains.supported() {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({
                    "error": "unsupported domain filter",
                    "supported_domains": SUPPORTED_STREAM_DOMAINS
                })
                .to_string(),
            ))
            .await;
        return;
    }

    let filter = EnvelopeFilter::from_query(&q);
    let mut quote_rx = domains.market_quote.then(|| state.bus.subscribe_quotes());
    let mut domain_rx = domains
        .single_event_domain()
        .map(|domain| state.bus.subscribe_domain(domain));
    let mut all_event_rx = domains
        .needs_mixed_event_bus()
        .then(|| state.bus.subscribe_events());
    let mut hb = interval(Duration::from_secs(15));
    let snapshot_interval_ms = q.snapshot_interval_ms.unwrap_or(1_000).clamp(250, 60_000);
    let mut snapshots = interval(Duration::from_millis(snapshot_interval_ms));

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if send_ws(&mut socket, Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = async {
                match &mut quote_rx {
                    Some(rx) => Some(rx.recv().await),
                    None => None,
                }
            }, if quote_rx.is_some() => {
                match msg {
                    Some(Ok(envelope)) => {
                        if !filter.matches(&envelope) {
                            continue;
                        }
                        if send_envelope(&mut socket, &envelope).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        warn!(skipped, "v1 stream consumer lagged behind quote bus");
                        continue;
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    None => {}
                }
            }
            msg = async {
                match &mut domain_rx {
                    Some(rx) => Some(rx.recv().await),
                    None => None,
                }
            }, if domain_rx.is_some() => {
                match msg {
                    Some(Ok(event))
                        if event_matches(event.as_ref(), &domains, &filter)
                            && send_event(&mut socket, event.as_ref()).await.is_err() =>
                    {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        warn!(skipped, "v1 stream consumer lagged behind domain bus");
                        continue;
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    None => {}
                }
            }
            msg = async {
                match &mut all_event_rx {
                    Some(rx) => Some(rx.recv().await),
                    None => None,
                }
            }, if all_event_rx.is_some() => {
                match msg {
                    Some(Ok(event))
                        if event_matches(event.as_ref(), &domains, &filter)
                            && send_event(&mut socket, event.as_ref()).await.is_err() =>
                    {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        warn!(skipped, "v1 stream consumer lagged behind all-event bus");
                        continue;
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    None => {}
                }
            }
            _ = snapshots.tick() => {
                if domains.options_chain {
                    let rows = state.deribit_cache
                        .filtered(DeribitOptionFilter {
                            include_stale: filter.include_stale,
                            ..Default::default()
                        })
                        .await;
                    for envelope in rows.into_iter().map(envelope_from_deribit_summary) {
                        if filter.matches(&envelope)
                            && send_envelope(&mut socket, &envelope).await.is_err()
                        {
                            return;
                        }
                    }
                }
                if domains.prediction_book {
                    let rows = state.polymarket_cache.all().await;
                    for envelope in rows.into_iter().map(envelope_from_polymarket_book) {
                        if filter.matches(&envelope)
                            && send_envelope(&mut socket, &envelope).await.is_err()
                        {
                            return;
                        }
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn ws_loop(
    mut socket: WebSocket,
    bus: EventBus,
    q: TickFilterQuery,
    metrics: Arc<AppMetrics>,
) {
    metrics.ws_subscribers.inc();

    let mut rx = bus.subscribe();
    let filter = TickFilter::from_query(q);
    let mut hb = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if send_ws(&mut socket, Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok(tick) => {
                        if !filter.matches(&tick) {
                            continue;
                        }
                        if send_json(&mut socket, &tick, "ws serialize failed")
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "ws consumer lagged behind event bus");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    metrics.ws_subscribers.dec();
}
