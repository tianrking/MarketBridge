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
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::core::schema::{DataEnvelope, ProductType};
use crate::deribit_cache::DeribitOptionFilter;
use crate::domains::options::chain::envelope_from_deribit_summary;
use crate::domains::prediction::book::envelope_from_polymarket_book;
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
    snapshot_interval_ms: Option<u64>,
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
    let domains = DomainFilter::from_query(q.domains);
    if !domains.supported() {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({
                    "error": "unsupported domain filter",
                    "supported_domains": ["market_quote", "options_chain", "prediction_book"]
                })
                .to_string(),
            ))
            .await;
        return;
    }

    let filter = EnvelopeFilter {
        symbols: q.symbols.map(parse_csv_set_upper),
        exchanges: q.exchanges.map(parse_csv_set_lower),
        product_type: q.product_type.map(|x| x.trim().to_ascii_lowercase()),
        include_stale: q.include_stale.unwrap_or(false),
    };
    let mut rx = state.bus.subscribe_quotes();
    let mut hb = interval(Duration::from_secs(15));
    let snapshot_interval_ms = q.snapshot_interval_ms.unwrap_or(1_000).clamp(250, 60_000);
    let mut snapshots = interval(Duration::from_millis(snapshot_interval_ms));

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
                        if !domains.market_quote || !filter.matches(&envelope) {
                            continue;
                        }
                        if send_envelope(&mut socket, &envelope).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "v1 stream consumer lagged behind quote bus");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
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
struct EnvelopeFilter {
    symbols: Option<HashSet<String>>,
    exchanges: Option<HashSet<String>>,
    product_type: Option<String>,
    include_stale: bool,
}

impl EnvelopeFilter {
    fn matches<T>(&self, envelope: &DataEnvelope<T>) -> bool {
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

#[derive(Debug, Default)]
struct DomainFilter {
    market_quote: bool,
    options_chain: bool,
    prediction_book: bool,
    unsupported: Vec<String>,
}

impl DomainFilter {
    fn from_query(raw: Option<String>) -> Self {
        let Some(raw) = raw else {
            return Self {
                market_quote: true,
                ..Default::default()
            };
        };
        let mut filter = Self::default();
        for domain in parse_csv_set_lower(raw) {
            match domain.as_str() {
                "market_quote" => filter.market_quote = true,
                "options_chain" => filter.options_chain = true,
                "prediction_book" => filter.prediction_book = true,
                _ => filter.unsupported.push(domain),
            }
        }
        filter
    }

    fn supported(&self) -> bool {
        self.unsupported.is_empty()
            && (self.market_quote || self.options_chain || self.prediction_book)
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

async fn send_envelope<T: serde::Serialize>(
    socket: &mut WebSocket,
    envelope: &DataEnvelope<T>,
) -> Result<(), ()> {
    match serde_json::to_string(envelope) {
        Ok(line) => socket.send(Message::Text(line)).await.map_err(|_| ()),
        Err(error) => {
            warn!(%error, "v1 stream serialize failed");
            Ok(())
        }
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
        let filter = EnvelopeFilter {
            symbols: Some(parse_csv_set_upper("BTCUSDT".to_string())),
            exchanges: Some(parse_csv_set_lower("okx".to_string())),
            product_type: Some("spot".to_string()),
            include_stale: false,
        };
        assert!(filter.matches(&envelope));
    }

    #[test]
    fn domain_filter_accepts_all_supported_stream_domains() {
        let filter = DomainFilter::from_query(Some(
            "market_quote,options_chain,prediction_book".to_string(),
        ));
        assert!(filter.supported());
        assert!(filter.market_quote);
        assert!(filter.options_chain);
        assert!(filter.prediction_book);
    }
}
