use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::time::interval;
use tracing::warn;

use crate::api::ApiState;
use crate::core::schema::{DataEnvelope, ProductType};
use crate::domains::market::quote::QuotePayload;
use crate::event_bus::{EventBus, NormalizedTick};
use crate::metrics::AppMetrics;

#[derive(Debug, Deserialize, Default)]
pub struct TickFilterQuery {
    pub symbols: Option<String>,
    pub exchanges: Option<String>,
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct V1StreamQuery {
    domains: Option<String>,
    symbols: Option<String>,
    exchanges: Option<String>,
    product_type: Option<String>,
    include_stale: Option<bool>,
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
    let bus = state.bus.clone();
    ws.on_upgrade(move |socket| v1_stream_loop(socket, bus, q))
}

async fn v1_stream_loop(mut socket: WebSocket, bus: EventBus, q: V1StreamQuery) {
    let domains = q.domains.map(parse_csv_set_lower);
    if domains
        .as_ref()
        .is_some_and(|set| !set.contains("market_quote"))
    {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({
                    "error": "unsupported domain filter",
                    "supported_domains": ["market_quote"]
                })
                .to_string(),
            ))
            .await;
        return;
    }

    let filter = EnvelopeQuoteFilter {
        symbols: q.symbols.map(parse_csv_set_upper),
        exchanges: q.exchanges.map(parse_csv_set_lower),
        product_type: q.product_type.map(|x| x.trim().to_ascii_lowercase()),
        include_stale: q.include_stale.unwrap_or(false),
    };
    let mut rx = bus.subscribe_quotes();
    let mut hb = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok(envelope) => {
                        if !filter.matches(&envelope) {
                            continue;
                        }
                        match serde_json::to_string(&envelope) {
                            Ok(line) => {
                                if socket.send(Message::Text(line)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => warn!(%error, "v1 stream serialize failed"),
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "v1 stream consumer lagged behind quote bus");
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
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok(tick) => {
                        if !filter.matches(&tick) {
                            continue;
                        }
                        match serde_json::to_string(&tick) {
                            Ok(line) => {
                                if socket.send(Message::Text(line)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(error=%e, "ws serialize failed");
                            }
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

#[derive(Default)]
struct TickFilter {
    symbols: Option<HashSet<String>>,
    exchanges: Option<HashSet<String>>,
    market: Option<String>,
}

#[derive(Default)]
struct EnvelopeQuoteFilter {
    symbols: Option<HashSet<String>>,
    exchanges: Option<HashSet<String>>,
    product_type: Option<String>,
    include_stale: bool,
}

impl EnvelopeQuoteFilter {
    fn matches(&self, envelope: &DataEnvelope<QuotePayload>) -> bool {
        if !self.include_stale && envelope.freshness.stale {
            return false;
        }
        if let Some(symbols) = &self.symbols
            && !envelope
                .instrument_ref
                .symbol
                .as_deref()
                .is_some_and(|symbol| symbols.contains(&symbol.to_ascii_uppercase()))
        {
            return false;
        }
        if let Some(exchanges) = &self.exchanges
            && !exchanges.contains(&envelope.source_ref.source.to_ascii_lowercase())
        {
            return false;
        }
        if let Some(product_type) = &self.product_type
            && !product_type_label(envelope.instrument_ref.product_type)
                .eq_ignore_ascii_case(product_type)
        {
            return false;
        }
        true
    }
}

impl TickFilter {
    fn from_query(q: TickFilterQuery) -> Self {
        Self {
            symbols: q.symbols.map(parse_csv_set_upper),
            exchanges: q.exchanges.map(parse_csv_set_lower),
            market: q.market.map(|x| x.trim().to_ascii_lowercase()),
        }
    }

    fn matches(&self, t: &NormalizedTick) -> bool {
        if let Some(symbols) = &self.symbols
            && !symbols.contains(&t.symbol.to_ascii_uppercase())
        {
            return false;
        }
        if let Some(exchanges) = &self.exchanges
            && !exchanges.contains(&t.exchange.to_ascii_lowercase())
        {
            return false;
        }
        if let Some(market) = &self.market
            && t.market != market
        {
            return false;
        }
        true
    }
}

fn parse_csv_set_upper(s: String) -> HashSet<String> {
    s.split(',')
        .map(|x| x.trim().to_ascii_uppercase())
        .filter(|x| !x.is_empty())
        .collect()
}

fn parse_csv_set_lower(s: String) -> HashSet<String> {
    s.split(',')
        .map(|x| x.trim().to_ascii_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

fn product_type_label(product_type: ProductType) -> &'static str {
    match product_type {
        ProductType::Spot => "spot",
        ProductType::Perp => "perp",
        ProductType::Future => "future",
        ProductType::Option => "option",
        ProductType::BinaryOutcome => "binary_outcome",
        ProductType::WalletTransfer => "wallet_transfer",
        ProductType::DexPool => "dex_pool",
        ProductType::Event => "event",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tick() -> NormalizedTick {
        NormalizedTick {
            version: "v1",
            exchange: "okx",
            market: "spot",
            symbol: "BTCUSDT".to_string(),
            bid: 1.0,
            ask: 2.0,
            mark: None,
            funding: None,
            ts: 1,
            source_latency_ms: 0,
            stale: false,
        }
    }

    #[test]
    fn filter_matches_symbol_exchange_market() {
        let q = TickFilterQuery {
            symbols: Some("BTCUSDT".to_string()),
            exchanges: Some("okx".to_string()),
            market: Some("spot".to_string()),
        };
        let f = TickFilter::from_query(q);
        assert!(f.matches(&sample_tick()));
    }

    #[test]
    fn filter_rejects_other_symbol() {
        let q = TickFilterQuery {
            symbols: Some("ETHUSDT".to_string()),
            exchanges: None,
            market: None,
        };
        let f = TickFilter::from_query(q);
        assert!(!f.matches(&sample_tick()));
    }

    #[test]
    fn envelope_filter_matches_quote() {
        let envelope = crate::domains::market::quote::envelope_from_tick(sample_tick());
        let filter = EnvelopeQuoteFilter {
            symbols: Some(parse_csv_set_upper("BTCUSDT".to_string())),
            exchanges: Some(parse_csv_set_lower("okx".to_string())),
            product_type: Some("spot".to_string()),
            include_stale: false,
        };
        assert!(filter.matches(&envelope));
    }
}
