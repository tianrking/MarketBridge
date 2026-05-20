use std::collections::HashSet;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use serde::Serialize;
use tokio::time::timeout;
use tracing::warn;

use crate::api::routes::stream::{TickFilterQuery, V1StreamQuery};
use crate::api::utils::{parse_csv_set_lower, parse_csv_set_upper};
use crate::core::schema::{DataEnvelope, ProductType};
use crate::event_bus::{EventDomain, NormalizedTick};
use crate::types::{DataEvent, MarketKind};

const WS_SEND_TIMEOUT: Duration = Duration::from_secs(3);

pub const SUPPORTED_STREAM_DOMAINS: [&str; 9] = [
    "market_quote",
    "funding",
    "open_interest",
    "trade",
    "liquidation",
    "order_book",
    "external_signal",
    "options_chain",
    "prediction_book",
];

#[derive(Default)]
pub struct TickFilter {
    symbols: Option<HashSet<String>>,
    exchanges: Option<HashSet<String>>,
    market: Option<String>,
}

#[derive(Default)]
pub struct EnvelopeFilter {
    symbols: Option<HashSet<String>>,
    exchanges: Option<HashSet<String>>,
    product_type: Option<String>,
    pub include_stale: bool,
}

#[derive(Debug, Default)]
pub struct StreamDomainFilter {
    pub market_quote: bool,
    pub funding: bool,
    pub open_interest: bool,
    pub trade: bool,
    pub liquidation: bool,
    pub order_book: bool,
    pub external_signal: bool,
    pub options_chain: bool,
    pub prediction_book: bool,
    unsupported: Vec<String>,
}

impl TickFilter {
    pub fn from_query(q: TickFilterQuery) -> Self {
        Self {
            symbols: q.symbols.map(parse_csv_set_upper),
            exchanges: q.exchanges.map(parse_csv_set_lower),
            market: q.market.map(|x| x.trim().to_ascii_lowercase()),
        }
    }

    pub fn matches(&self, t: &NormalizedTick) -> bool {
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

impl EnvelopeFilter {
    pub fn from_query(q: &V1StreamQuery) -> Self {
        Self {
            symbols: q.symbols.as_ref().map(|x| parse_csv_set_upper(x.clone())),
            exchanges: q.exchanges.as_ref().map(|x| parse_csv_set_lower(x.clone())),
            product_type: q
                .product_type
                .as_ref()
                .map(|x| x.trim().to_ascii_lowercase()),
            include_stale: q.include_stale.unwrap_or(false),
        }
    }

    pub fn matches<T>(&self, envelope: &DataEnvelope<T>) -> bool {
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

impl StreamDomainFilter {
    pub fn from_query(raw: Option<String>) -> Self {
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

    pub fn supported(&self) -> bool {
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

    pub fn single_event_domain(&self) -> Option<EventDomain> {
        let selected = self.selected_event_domains();
        if selected.len() == 1 && !self.market_quote && !self.options_chain && !self.prediction_book
        {
            selected.first().copied()
        } else {
            None
        }
    }

    pub fn needs_mixed_event_bus(&self) -> bool {
        self.single_event_domain().is_none() && !self.selected_event_domains().is_empty()
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
}

pub fn event_matches(
    event: &DataEvent,
    domains: &StreamDomainFilter,
    filter: &EnvelopeFilter,
) -> bool {
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

pub async fn send_event(socket: &mut WebSocket, event: &DataEvent) -> Result<(), ()> {
    send_json(socket, event, "v1 stream event serialize failed").await
}

pub async fn send_envelope<T: Serialize>(
    socket: &mut WebSocket,
    envelope: &DataEnvelope<T>,
) -> Result<(), ()> {
    send_json(socket, envelope, "v1 stream serialize failed").await
}

pub async fn send_json<T: Serialize>(
    socket: &mut WebSocket,
    value: &T,
    error_message: &'static str,
) -> Result<(), ()> {
    match serde_json::to_string(value) {
        Ok(line) => send_ws(socket, Message::Text(line)).await,
        Err(error) => {
            warn!(%error, error_message);
            Ok(())
        }
    }
}

pub async fn send_ws(socket: &mut WebSocket, message: Message) -> Result<(), ()> {
    match timeout(WS_SEND_TIMEOUT, socket.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(()),
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
        let filter = StreamDomainFilter::from_query(Some(SUPPORTED_STREAM_DOMAINS.join(",")));
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
        let filter = StreamDomainFilter::from_query(Some("funding".to_string()));

        assert_eq!(filter.single_event_domain(), Some(EventDomain::Funding));
        assert!(!filter.needs_mixed_event_bus());
    }
}
