use std::sync::Arc;

use tokio::sync::broadcast;

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::QuotePayload;
use crate::event_snapshots::EventSnapshotStore;
pub use crate::event_snapshots::NormalizedTick;
use crate::types::{
    DataEvent, ExternalSignalTick, FundingRateTick, LiquidationTick, OpenInterestTick,
    OrderBookTick, TradeTick,
};

#[derive(Debug, Clone)]
pub struct SharedEvent {
    pub event: Arc<DataEvent>,
    pub json: Arc<str>,
}

impl SharedEvent {
    pub fn new(event: Arc<DataEvent>) -> Self {
        let json = serde_json::to_string(event.as_ref())
            .unwrap_or_else(|error| {
                serde_json::json!({
                    "type": "serialization_error",
                    "error": error.to_string(),
                })
                .to_string()
            })
            .into();
        Self { event, json }
    }
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<NormalizedTick>,
    event_tx: broadcast::Sender<Arc<SharedEvent>>,
    quote_tx: broadcast::Sender<DataEnvelope<QuotePayload>>,
    funding_tx: broadcast::Sender<Arc<SharedEvent>>,
    open_interest_tx: broadcast::Sender<Arc<SharedEvent>>,
    trade_tx: broadcast::Sender<Arc<SharedEvent>>,
    liquidation_tx: broadcast::Sender<Arc<SharedEvent>>,
    order_book_tx: broadcast::Sender<Arc<SharedEvent>>,
    external_signal_tx: broadcast::Sender<Arc<SharedEvent>>,
    snapshots: EventSnapshotStore,
    stale_ttl_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDomain {
    Funding,
    OpenInterest,
    Trade,
    Liquidation,
    OrderBook,
    ExternalSignal,
}

impl EventBus {
    pub fn new(capacity: usize, stale_ttl_ms: u64) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        let (event_tx, _) = broadcast::channel(capacity);
        let (quote_tx, _) = broadcast::channel(capacity);
        let (funding_tx, _) = broadcast::channel(capacity);
        let (open_interest_tx, _) = broadcast::channel(capacity);
        let (trade_tx, _) = broadcast::channel(capacity);
        let (liquidation_tx, _) = broadcast::channel(capacity);
        let (order_book_tx, _) = broadcast::channel(capacity);
        let (external_signal_tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            event_tx,
            quote_tx,
            funding_tx,
            open_interest_tx,
            trade_tx,
            liquidation_tx,
            order_book_tx,
            external_signal_tx,
            snapshots: EventSnapshotStore::new(),
            stale_ttl_ms,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedTick> {
        self.tx.subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Arc<SharedEvent>> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_quotes(&self) -> broadcast::Receiver<DataEnvelope<QuotePayload>> {
        self.quote_tx.subscribe()
    }

    pub fn subscribe_domain(&self, domain: EventDomain) -> broadcast::Receiver<Arc<SharedEvent>> {
        match domain {
            EventDomain::Funding => self.funding_tx.subscribe(),
            EventDomain::OpenInterest => self.open_interest_tx.subscribe(),
            EventDomain::Trade => self.trade_tx.subscribe(),
            EventDomain::Liquidation => self.liquidation_tx.subscribe(),
            EventDomain::OrderBook => self.order_book_tx.subscribe(),
            EventDomain::ExternalSignal => self.external_signal_tx.subscribe(),
        }
    }

    #[cfg(test)]
    pub fn publish_from_event(&self, event: &DataEvent) {
        self.publish_shared_event(Arc::new(event.clone()));
    }

    pub fn publish_shared_event(&self, event: Arc<DataEvent>) {
        let shared = Arc::new(SharedEvent::new(event));
        let _ = self.event_tx.send(shared.clone());
        match shared.event.as_ref() {
            DataEvent::Tick(t) => {
                let (normalized, quote_envelope) = self.snapshots.upsert_tick(t, self.stale_ttl_ms);
                let _ = self.tx.send(normalized);
                let _ = self.quote_tx.send(quote_envelope);
            }
            DataEvent::FundingRate(t) => {
                let _ = self.funding_tx.send(shared.clone());
                self.snapshots.upsert_funding(t);
            }
            DataEvent::OpenInterest(t) => {
                let _ = self.open_interest_tx.send(shared.clone());
                self.snapshots.upsert_open_interest(t);
            }
            DataEvent::Trade(t) => {
                let _ = self.trade_tx.send(shared.clone());
                self.snapshots.upsert_trade(t);
            }
            DataEvent::Liquidation(t) => {
                let _ = self.liquidation_tx.send(shared.clone());
                self.snapshots.upsert_liquidation(t);
            }
            DataEvent::OrderBook(t) => {
                let _ = self.order_book_tx.send(shared.clone());
                self.snapshots.upsert_order_book(t);
            }
            DataEvent::ExternalSignal(t) => {
                let _ = self.external_signal_tx.send(shared.clone());
                self.snapshots.upsert_external_signal(t);
            }
            DataEvent::Heartbeat { .. } => {}
        }
    }

    pub async fn snapshot_by_symbol(&self, symbol: &str) -> Vec<NormalizedTick> {
        self.snapshots.snapshot_by_symbol(symbol).await
    }

    pub async fn snapshot_all(&self) -> Vec<NormalizedTick> {
        self.snapshots.snapshot_all().await
    }

    pub async fn quote_snapshot_all(&self) -> Vec<DataEnvelope<QuotePayload>> {
        self.snapshots.quote_snapshot_all().await
    }

    pub async fn funding_snapshot_all(&self) -> Vec<FundingRateTick> {
        self.snapshots.funding_snapshot_all().await
    }

    pub async fn open_interest_snapshot_all(&self) -> Vec<OpenInterestTick> {
        self.snapshots.open_interest_snapshot_all().await
    }

    pub async fn trade_snapshot_all(&self) -> Vec<TradeTick> {
        self.snapshots.trade_snapshot_all().await
    }

    pub async fn liquidation_snapshot_all(&self) -> Vec<LiquidationTick> {
        self.snapshots.liquidation_snapshot_all().await
    }

    pub async fn order_book_snapshot_all(&self) -> Vec<OrderBookTick> {
        self.snapshots.order_book_snapshot_all().await
    }

    pub async fn external_signal_snapshot_all(&self) -> Vec<ExternalSignalTick> {
        self.snapshots.external_signal_snapshot_all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        BookLevel, ExternalSignalTick, FundingRateTick, LiquidationTick, MarketKind, MarketTick,
        OpenInterestTick, OrderBookTick, TradeSide, TradeTick, now_ms,
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
        bus.publish_from_event(&DataEvent::Tick(tick));
        let all = bus.snapshot_all().await;
        assert_eq!(all.len(), 1);
        let t = &all[0];
        assert_eq!(t.exchange, "okx");
        assert_eq!(t.market, "spot");
        assert_eq!(t.symbol, "BTCUSDT");
        let mut rx = bus.subscribe_events();
        bus.publish_from_event(&DataEvent::Heartbeat {
            exchange: "okx",
            ts_ms: now_ms(),
        });
        let received = rx.recv().await.expect("heartbeat event should publish");
        assert!(matches!(
            received.event.as_ref(),
            DataEvent::Heartbeat {
                exchange: "okx",
                ..
            }
        ));
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
        }));
        bus.publish_from_event(&DataEvent::OpenInterest(OpenInterestTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            open_interest: 10.0,
            open_interest_value: Some(1000.0),
            ts_ms,
        }));
        bus.publish_from_event(&DataEvent::Trade(TradeTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 2.0,
            side: TradeSide::Buy,
            trade_id: Some("1".into()),
            ts_ms,
        }));
        bus.publish_from_event(&DataEvent::Liquidation(LiquidationTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            side: TradeSide::Sell,
            price: 90.0,
            qty: 1.0,
            ts_ms,
        }));
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
        }));

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
        }));
        assert_eq!(bus.external_signal_snapshot_all().await.len(), 1);
    }

    #[tokio::test]
    async fn domain_subscribers_only_receive_matching_events() {
        let bus = EventBus::new(16, 1000);
        let mut funding_rx = bus.subscribe_domain(EventDomain::Funding);
        let mut trade_rx = bus.subscribe_domain(EventDomain::Trade);
        let ts_ms = now_ms();

        bus.publish_from_event(&DataEvent::FundingRate(FundingRateTick {
            exchange: "binance",
            symbol: "BTCUSDT".into(),
            funding_rate: 0.0001,
            next_funding_time_ms: None,
            mark_price: None,
            index_price: None,
            ts_ms,
        }));

        let received = funding_rx.recv().await.expect("funding event");
        assert!(matches!(received.event.as_ref(), DataEvent::FundingRate(_)));
        assert!(trade_rx.try_recv().is_err());
    }
}
