use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{RwLock, broadcast};

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::{QuotePayload, envelope_from_tick};
use crate::types::{
    DataEvent, ExternalSignalTick, FundingRateTick, LiquidationTick, MarketKind, OpenInterestTick,
    OrderBookTick, TradeTick, now_ms,
};

pub const SCHEMA_VERSION: &str = "v1";

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedTick {
    pub version: &'static str,
    pub exchange: &'static str,
    pub market: &'static str,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub mark: Option<f64>,
    pub funding: Option<f64>,
    pub ts: u64,
    pub source_latency_ms: u64,
    pub stale: bool,
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<NormalizedTick>,
    quote_tx: broadcast::Sender<DataEnvelope<QuotePayload>>,
    snapshots: Arc<RwLock<HashMap<String, NormalizedTick>>>,
    quote_snapshots: Arc<RwLock<HashMap<String, DataEnvelope<QuotePayload>>>>,
    funding_snapshots: Arc<RwLock<HashMap<String, FundingRateTick>>>,
    open_interest_snapshots: Arc<RwLock<HashMap<String, OpenInterestTick>>>,
    trade_snapshots: Arc<RwLock<HashMap<String, TradeTick>>>,
    liquidation_snapshots: Arc<RwLock<HashMap<String, LiquidationTick>>>,
    order_book_snapshots: Arc<RwLock<HashMap<String, OrderBookTick>>>,
    external_signal_snapshots: Arc<RwLock<HashMap<String, ExternalSignalTick>>>,
    stale_ttl_ms: u64,
}

impl EventBus {
    pub fn new(capacity: usize, stale_ttl_ms: u64) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        let (quote_tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            quote_tx,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            quote_snapshots: Arc::new(RwLock::new(HashMap::new())),
            funding_snapshots: Arc::new(RwLock::new(HashMap::new())),
            open_interest_snapshots: Arc::new(RwLock::new(HashMap::new())),
            trade_snapshots: Arc::new(RwLock::new(HashMap::new())),
            liquidation_snapshots: Arc::new(RwLock::new(HashMap::new())),
            order_book_snapshots: Arc::new(RwLock::new(HashMap::new())),
            external_signal_snapshots: Arc::new(RwLock::new(HashMap::new())),
            stale_ttl_ms,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedTick> {
        self.tx.subscribe()
    }

    pub fn subscribe_quotes(&self) -> broadcast::Receiver<DataEnvelope<QuotePayload>> {
        self.quote_tx.subscribe()
    }

    pub async fn publish_from_event(&self, event: &DataEvent) {
        match event {
            DataEvent::Tick(t) => {
                let now = now_ms();
                let latency = now.saturating_sub(t.ts_ms);
                let normalized = NormalizedTick {
                    version: SCHEMA_VERSION,
                    exchange: t.exchange,
                    market: market_to_str(t.market),
                    symbol: t.symbol.to_string(),
                    bid: t.bid,
                    ask: t.ask,
                    mark: t.mark,
                    funding: t.funding_rate,
                    ts: t.ts_ms,
                    source_latency_ms: latency,
                    stale: latency > self.stale_ttl_ms,
                };

                let key = snapshot_key(&normalized.exchange, normalized.market, &normalized.symbol);
                let quote_envelope = envelope_from_tick(normalized.clone());
                let quote_key = quote_snapshot_key(&quote_envelope);
                {
                    let mut guard = self.snapshots.write().await;
                    guard.insert(key, normalized.clone());
                }
                {
                    let mut guard = self.quote_snapshots.write().await;
                    guard.insert(quote_key, quote_envelope.clone());
                }
                let _ = self.tx.send(normalized);
                let _ = self.quote_tx.send(quote_envelope);
            }
            DataEvent::FundingRate(t) => {
                self.funding_snapshots
                    .write()
                    .await
                    .insert(perp_key(t.exchange, &t.symbol), t.clone());
            }
            DataEvent::OpenInterest(t) => {
                self.open_interest_snapshots
                    .write()
                    .await
                    .insert(perp_key(t.exchange, &t.symbol), t.clone());
            }
            DataEvent::Trade(t) => {
                self.trade_snapshots
                    .write()
                    .await
                    .insert(market_key(t.exchange, t.market, &t.symbol), t.clone());
            }
            DataEvent::Liquidation(t) => {
                self.liquidation_snapshots
                    .write()
                    .await
                    .insert(perp_key(t.exchange, &t.symbol), t.clone());
            }
            DataEvent::OrderBook(t) => {
                self.order_book_snapshots
                    .write()
                    .await
                    .insert(market_key(t.exchange, t.market, &t.symbol), t.clone());
            }
            DataEvent::ExternalSignal(t) => {
                self.external_signal_snapshots
                    .write()
                    .await
                    .insert(external_signal_key(t), t.clone());
            }
            DataEvent::Heartbeat { .. } => {}
        }
    }

    pub async fn snapshot_by_symbol(&self, symbol: &str) -> Vec<NormalizedTick> {
        let needle = symbol.to_ascii_uppercase();
        let guard = self.snapshots.read().await;
        guard
            .values()
            .filter(|t| t.symbol.eq_ignore_ascii_case(&needle))
            .cloned()
            .collect()
    }

    pub async fn snapshot_all(&self) -> Vec<NormalizedTick> {
        let guard = self.snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn quote_snapshot_all(&self) -> Vec<DataEnvelope<QuotePayload>> {
        let guard = self.quote_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn funding_snapshot_all(&self) -> Vec<FundingRateTick> {
        let guard = self.funding_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn open_interest_snapshot_all(&self) -> Vec<OpenInterestTick> {
        let guard = self.open_interest_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn trade_snapshot_all(&self) -> Vec<TradeTick> {
        let guard = self.trade_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn liquidation_snapshot_all(&self) -> Vec<LiquidationTick> {
        let guard = self.liquidation_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn order_book_snapshot_all(&self) -> Vec<OrderBookTick> {
        let guard = self.order_book_snapshots.read().await;
        guard.values().cloned().collect()
    }

    pub async fn external_signal_snapshot_all(&self) -> Vec<ExternalSignalTick> {
        let guard = self.external_signal_snapshots.read().await;
        guard.values().cloned().collect()
    }
}

fn market_to_str(m: MarketKind) -> &'static str {
    match m {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

fn snapshot_key(exchange: &str, market: &str, symbol: &str) -> String {
    format!("{exchange}:{market}:{symbol}")
}

fn perp_key(exchange: &str, symbol: &str) -> String {
    format!("{exchange}:perp:{symbol}")
}

fn market_key(exchange: &str, market: MarketKind, symbol: &str) -> String {
    format!("{exchange}:{}:{symbol}", market_to_str(market))
}

fn quote_snapshot_key(envelope: &DataEnvelope<QuotePayload>) -> String {
    format!(
        "{}:{}",
        envelope.source_ref.source, envelope.instrument_ref.instrument_id
    )
}

fn external_signal_key(t: &ExternalSignalTick) -> String {
    format!(
        "{}:{}:{}:{}",
        t.source,
        t.category,
        t.symbol.as_deref().unwrap_or("*"),
        t.metric
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        BookLevel, ExternalSignalTick, FundingRateTick, LiquidationTick, MarketKind, MarketTick,
        OpenInterestTick, OrderBookTick, TradeSide, TradeTick,
    };

    #[tokio::test]
    async fn publish_builds_normalized_tick() {
        let bus = EventBus::new(16, 1000);
        let tick = MarketTick {
            exchange: "okx",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bid: 1.0,
            ask: 2.0,
            mark: None,
            funding_rate: None,
            ts_ms: now_ms(),
        };
        bus.publish_from_event(&DataEvent::Tick(tick)).await;
        let all = bus.snapshot_all().await;
        assert_eq!(all.len(), 1);
        let t = &all[0];
        assert_eq!(t.exchange, "okx");
        assert_eq!(t.market, "spot");
        assert_eq!(t.symbol, "BTCUSDT");
        assert!(t.source_latency_ms <= 1000);
        let quotes = bus.quote_snapshot_all().await;
        assert_eq!(quotes.len(), 1);
        assert_eq!(
            quotes[0].domain,
            crate::core::schema::DataDomain::MarketQuote
        );
    }

    #[tokio::test]
    async fn publish_stores_extended_market_events() {
        let bus = EventBus::new(16, 1000);
        let ts_ms = now_ms();
        bus.publish_from_event(&DataEvent::FundingRate(FundingRateTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            funding_rate: 0.0001,
            next_funding_time_ms: Some(ts_ms + 1000),
            mark_price: Some(100.0),
            index_price: Some(99.0),
            ts_ms,
        }))
        .await;
        bus.publish_from_event(&DataEvent::OpenInterest(OpenInterestTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            open_interest: 10.0,
            open_interest_value: Some(1000.0),
            ts_ms,
        }))
        .await;
        bus.publish_from_event(&DataEvent::Trade(TradeTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 2.0,
            side: TradeSide::Buy,
            trade_id: Some("1".into()),
            ts_ms,
        }))
        .await;
        bus.publish_from_event(&DataEvent::Liquidation(LiquidationTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            side: TradeSide::Sell,
            price: 90.0,
            qty: 1.0,
            ts_ms,
        }))
        .await;
        bus.publish_from_event(&DataEvent::OrderBook(OrderBookTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            bids: vec![BookLevel {
                price: 99.0,
                qty: 1.0,
            }],
            asks: vec![BookLevel {
                price: 101.0,
                qty: 1.0,
            }],
            last_update_id: Some(7),
            ts_ms,
        }))
        .await;

        assert_eq!(bus.funding_snapshot_all().await.len(), 1);
        assert_eq!(bus.open_interest_snapshot_all().await.len(), 1);
        assert_eq!(bus.trade_snapshot_all().await.len(), 1);
        assert_eq!(bus.liquidation_snapshot_all().await.len(), 1);
        assert_eq!(bus.order_book_snapshot_all().await.len(), 1);
        bus.publish_from_event(&DataEvent::ExternalSignal(ExternalSignalTick {
            source: "fear_greed",
            category: "sentiment".into(),
            symbol: Some("BTC".into()),
            metric: "fear_greed_index".into(),
            value: Some(50.0),
            score: Some(50.0),
            title: None,
            url: None,
            ts_ms: now_ms(),
            raw: None,
        }))
        .await;
        assert_eq!(bus.external_signal_snapshot_all().await.len(), 1);
    }
}
