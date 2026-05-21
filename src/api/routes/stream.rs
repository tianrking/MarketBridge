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
    send_json, send_shared_event, send_shared_snapshot, send_ws, snapshot_domain_matches,
};
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
    let mut options_snapshot_rx = domains
        .options_chain
        .then(|| state.snapshot_stream_hub.subscribe_options());
    let mut prediction_snapshot_rx = domains
        .prediction_book
        .then(|| state.snapshot_stream_hub.subscribe_prediction());
    let mut hb = interval(Duration::from_secs(15));
    let _requested_snapshot_interval_ms =
        q.snapshot_interval_ms.unwrap_or(1_000).clamp(250, 60_000);

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if let Err(error) = send_ws(&mut socket, Message::Ping(vec![])).await {
                    warn!(%error, "v1 stream ping failed");
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
                        if let Err(error) = crate::api::streaming::send_envelope(&mut socket, &envelope).await {
                            warn!(%error, "v1 stream quote send failed");
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
                    Some(Ok(event)) if event_matches(event.event.as_ref(), &domains, &filter) => {
                        if let Err(error) = send_shared_event(&mut socket, event.as_ref()).await {
                            warn!(%error, "v1 stream domain event send failed");
                            break;
                        }
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
                    Some(Ok(event)) if event_matches(event.event.as_ref(), &domains, &filter) => {
                        if let Err(error) = send_shared_event(&mut socket, event.as_ref()).await {
                            warn!(%error, "v1 stream mixed event send failed");
                            break;
                        }
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
            msg = async {
                match &mut options_snapshot_rx {
                    Some(rx) => Some(rx.recv().await),
                    None => None,
                }
            }, if options_snapshot_rx.is_some() => {
                match msg {
                    Some(Ok(snapshot))
                        if snapshot_domain_matches(snapshot.as_ref(), &domains)
                            && filter.matches_snapshot(snapshot.as_ref()) =>
                    {
                        if let Err(error) = send_shared_snapshot(&mut socket, snapshot.as_ref()).await {
                            warn!(%error, "v1 stream options snapshot send failed");
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        warn!(skipped, "v1 stream consumer lagged behind options snapshot bus");
                        continue;
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    None => {}
                }
            }
            msg = async {
                match &mut prediction_snapshot_rx {
                    Some(rx) => Some(rx.recv().await),
                    None => None,
                }
            }, if prediction_snapshot_rx.is_some() => {
                match msg {
                    Some(Ok(snapshot))
                        if snapshot_domain_matches(snapshot.as_ref(), &domains)
                            && filter.matches_snapshot(snapshot.as_ref()) =>
                    {
                        if let Err(error) = send_shared_snapshot(&mut socket, snapshot.as_ref()).await {
                            warn!(%error, "v1 stream prediction snapshot send failed");
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                        warn!(skipped, "v1 stream consumer lagged behind prediction snapshot bus");
                        continue;
                    }
                    Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                    None => {}
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
                if let Err(error) = send_ws(&mut socket, Message::Ping(vec![])).await {
                    warn!(%error, "legacy tick stream ping failed");
                    break;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok(tick) => {
                        if !filter.matches(&tick) {
                            continue;
                        }
                        if let Err(error) = send_json(&mut socket, &tick, "ws serialize failed").await {
                            warn!(%error, "legacy tick stream send failed");
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
