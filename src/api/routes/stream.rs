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
use crate::event_bus::{EventBus, EventDomain, NormalizedTick};
use crate::metrics::AppMetrics;
use crate::types::{DataEvent, MarketKind};

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
                    "supported_domains": [
                        "market_quote",
                        "funding",
                        "open_interest",
                        "trade",
                        "liquidation",
                        "order_book",
                        "external_signal",
                        "options_chain",
                        "prediction_book"
                    ]
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
                if socket.send(Message::Ping(vec![])).await.is_err() {
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
                    Some(Ok(event)) => {
                        if event_matches(&event, &domains, &filter)
                            && send_event(&mut socket, &event).await.is_err()
                        {
                            break;
                        }
                    }
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
                    Some(Ok(event)) => {
                        if event_matches(&event, &domains, &filter)
                            && send_event(&mut socket, &event).await.is_err()
                        {
                            break;
                        }
                    }
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

    fn matches_raw(&self, exchange: &str, market: MarketKind, symbol: &str) -> bool {
        if let Some(symbols) = &self.symbols
            && !symbols.contains(&symbol.to_ascii_uppercase())
        {
            return false;
        }
        if let Some(exchanges) = &self.exchanges
            && !exchanges.contains(&exchange.to_ascii_lowercase())
        {
            return false;
        }
        if let Some(product_type) = &self.product_type
            && market_kind_label(market) != product_type
        {
            return false;
        }
        true
    }

    fn matches_external(&self, source: &str, symbol: Option<&str>) -> bool {
        if let Some(symbols) = &self.symbols {
            let Some(symbol) = symbol else {
                return false;
            };
            if !symbols.contains(&symbol.to_ascii_uppercase()) {
                return false;
            }
        }
        if let Some(exchanges) = &self.exchanges
            && !exchanges.contains(&source.to_ascii_lowercase())
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Default)]
struct DomainFilter {
    market_quote: bool,
    funding: bool,
    open_interest: bool,
    trade: bool,
    liquidation: bool,
    order_book: bool,
    external_signal: bool,
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
                "funding" => filter.funding = true,
                "open_interest" => filter.open_interest = true,
                "trade" => filter.trade = true,
                "liquidation" => filter.liquidation = true,
                "order_book" => filter.order_book = true,
                "external_signal" => filter.external_signal = true,
                "options_chain" => filter.options_chain = true,
                "prediction_book" => filter.prediction_book = true,
                _ => filter.unsupported.push(domain),
            }
        }
        filter
    }

    fn supported(&self) -> bool {
        self.unsupported.is_empty()
            && (self.market_quote
                || self.funding
                || self.open_interest
                || self.trade
                || self.liquidation
                || self.order_book
                || self.external_signal
                || self.options_chain
                || self.prediction_book)
    }

    fn selected_event_domains(&self) -> Vec<EventDomain> {
        let mut out = Vec::new();
        if self.funding {
            out.push(EventDomain::Funding);
        }
        if self.open_interest {
            out.push(EventDomain::OpenInterest);
        }
        if self.trade {
            out.push(EventDomain::Trade);
        }
        if self.liquidation {
            out.push(EventDomain::Liquidation);
        }
        if self.order_book {
            out.push(EventDomain::OrderBook);
        }
        if self.external_signal {
            out.push(EventDomain::ExternalSignal);
        }
        out
    }

    fn single_event_domain(&self) -> Option<EventDomain> {
        let selected = self.selected_event_domains();
        if selected.len() == 1 && !self.market_quote && !self.options_chain && !self.prediction_book
        {
            selected.first().copied()
        } else {
            None
        }
    }

    fn needs_mixed_event_bus(&self) -> bool {
        self.single_event_domain().is_none() && !self.selected_event_domains().is_empty()
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

fn market_kind_label(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

fn event_matches(event: &DataEvent, domains: &DomainFilter, filter: &EnvelopeFilter) -> bool {
    match event {
        DataEvent::FundingRate(t) => {
            domains.funding && filter.matches_raw(t.exchange, MarketKind::Perp, &t.symbol)
        }
        DataEvent::OpenInterest(t) => {
            domains.open_interest && filter.matches_raw(t.exchange, MarketKind::Perp, &t.symbol)
        }
        DataEvent::Trade(t) => domains.trade && filter.matches_raw(t.exchange, t.market, &t.symbol),
        DataEvent::Liquidation(t) => {
            domains.liquidation && filter.matches_raw(t.exchange, MarketKind::Perp, &t.symbol)
        }
        DataEvent::OrderBook(t) => {
            domains.order_book && filter.matches_raw(t.exchange, t.market, &t.symbol)
        }
        DataEvent::ExternalSignal(t) => {
            domains.external_signal && filter.matches_external(t.source, t.symbol.as_deref())
        }
        DataEvent::Tick(_) | DataEvent::Heartbeat { .. } => false,
    }
}

async fn send_event(socket: &mut WebSocket, event: &DataEvent) -> Result<(), ()> {
    match serde_json::to_string(event) {
        Ok(line) => socket.send(Message::Text(line)).await.map_err(|_| ()),
        Err(error) => {
            warn!(%error, "v1 stream event serialize failed");
            Ok(())
        }
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
            "market_quote,funding,open_interest,trade,liquidation,order_book,external_signal,options_chain,prediction_book".to_string(),
        ));
        assert!(filter.supported());
        assert!(filter.market_quote);
        assert!(filter.funding);
        assert!(filter.open_interest);
        assert!(filter.trade);
        assert!(filter.liquidation);
        assert!(filter.order_book);
        assert!(filter.external_signal);
        assert!(filter.options_chain);
        assert!(filter.prediction_book);
    }

    #[test]
    fn domain_filter_uses_single_domain_fast_path() {
        let filter = DomainFilter::from_query(Some("funding".to_string()));

        assert_eq!(filter.single_event_domain(), Some(EventDomain::Funding));
        assert!(!filter.needs_mixed_event_bus());
    }
}
