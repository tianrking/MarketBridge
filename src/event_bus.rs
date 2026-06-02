use std::sync::Arc;
use std::sync::OnceLock;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::QuotePayload;
use crate::event_snapshots::EventSnapshotStore;
pub use crate::event_snapshots::NormalizedTick;
use crate::types::{
    DataEvent, ExternalSignalTick, FundingRateTick, LiquidationTick, OpenInterestTick,
    OrderBookTick, TradeTick,
};

#[derive(Debug)]
pub struct SharedTick {
    pub tick: Arc<NormalizedTick>,
    json: OnceLock<Arc<str>>,
}

impl SharedTick {
    pub fn new(tick: NormalizedTick) -> Self {
        Self {
            tick: Arc::new(tick),
            json: OnceLock::new(),
        }
    }

    pub fn json(&self) -> Arc<str> {
        self.json
            .get_or_init(|| serialize_json(self.tick.as_ref()))
            .clone()
    }
}

#[derive(Debug)]
pub struct SharedEvent {
    pub event: Arc<DataEvent>,
    json: OnceLock<Arc<str>>,
}

impl SharedEvent {
    pub fn new(event: Arc<DataEvent>) -> Self {
        Self {
            event,
            json: OnceLock::new(),
        }
    }

    pub fn json(&self) -> Arc<str> {
        self.json
            .get_or_init(|| serialize_event_json(self.event.as_ref()))
            .clone()
    }
}

#[derive(Debug)]
pub struct SharedQuote {
    pub envelope: Arc<DataEnvelope<QuotePayload>>,
    json: OnceLock<Arc<str>>,
}

impl SharedQuote {
    pub fn new(envelope: DataEnvelope<QuotePayload>) -> Self {
        Self {
            envelope: Arc::new(envelope),
            json: OnceLock::new(),
        }
    }

    pub fn json(&self) -> Arc<str> {
        self.json
            .get_or_init(|| serialize_json(self.envelope.as_ref()))
            .clone()
    }
}

fn serialize_event_json(event: &DataEvent) -> Arc<str> {
    serialize_json(event)
}

fn serialize_json<T: serde::Serialize>(value: &T) -> Arc<str> {
    serde_json::to_string(value)
        .unwrap_or_else(|error| {
            warn!(%error, "event json serialization failed");
            serde_json::json!({
                "type": "serialization_error",
                "error": error.to_string(),
            })
            .to_string()
        })
        .into()
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Arc<SharedTick>>,
    event_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    quote_tx: broadcast::Sender<Arc<SharedQuote>>,
    funding_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    open_interest_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    trade_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    liquidation_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    order_book_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    external_signal_tx: Vec<broadcast::Sender<Arc<SharedEvent>>>,
    snapshots: EventSnapshotStore,
    stale_ttl_ms: u64,
}

pub struct EventSubscription {
    rx: mpsc::Receiver<Result<Arc<SharedEvent>, broadcast::error::RecvError>>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl EventSubscription {
    pub async fn recv(&mut self) -> Result<Arc<SharedEvent>, broadcast::error::RecvError> {
        self.rx
            .recv()
            .await
            .unwrap_or(Err(broadcast::error::RecvError::Closed))
    }

    #[cfg(test)]
    pub fn try_recv(
        &mut self,
    ) -> Result<Result<Arc<SharedEvent>, broadcast::error::RecvError>, mpsc::error::TryRecvError>
    {
        self.rx.try_recv()
    }
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
    pub fn new_sharded(capacity: usize, stale_ttl_ms: u64, event_shards: usize) -> Self {
        let event_shards = event_shards.max(1);
        let (tx, _) = broadcast::channel(capacity);
        let (quote_tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            event_tx: sharded_senders(capacity, event_shards),
            quote_tx,
            funding_tx: sharded_senders(capacity, event_shards),
            open_interest_tx: sharded_senders(capacity, event_shards),
            trade_tx: sharded_senders(capacity, event_shards),
            liquidation_tx: sharded_senders(capacity, event_shards),
            order_book_tx: sharded_senders(capacity, event_shards),
            external_signal_tx: sharded_senders(capacity, event_shards),
            snapshots: EventSnapshotStore::new(),
            stale_ttl_ms,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<SharedTick>> {
        self.tx.subscribe()
    }

    pub fn subscribe_events(&self) -> EventSubscription {
        subscribe_sharded(&self.event_tx)
    }

    pub fn subscribe_quotes(&self) -> broadcast::Receiver<Arc<SharedQuote>> {
        self.quote_tx.subscribe()
    }

    pub fn subscribe_domain(&self, domain: EventDomain) -> EventSubscription {
        match domain {
            EventDomain::Funding => subscribe_sharded(&self.funding_tx),
            EventDomain::OpenInterest => subscribe_sharded(&self.open_interest_tx),
            EventDomain::Trade => subscribe_sharded(&self.trade_tx),
            EventDomain::Liquidation => subscribe_sharded(&self.liquidation_tx),
            EventDomain::OrderBook => subscribe_sharded(&self.order_book_tx),
            EventDomain::ExternalSignal => subscribe_sharded(&self.external_signal_tx),
        }
    }

    #[cfg(test)]
    pub fn publish_from_event(&self, event: &DataEvent) {
        self.publish_shared_event(Arc::new(event.clone()));
    }

    pub fn publish_shared_event(&self, event: Arc<DataEvent>) {
        let shared = Arc::new(SharedEvent::new(event));
        let shard = shard_for_event(shared.event.as_ref(), self.event_tx.len());
        let _ = self.event_tx[shard].send(shared.clone());
        match shared.event.as_ref() {
            DataEvent::Tick(t) => {
                let (normalized, quote_envelope) = self.snapshots.upsert_tick(t, self.stale_ttl_ms);
                if self.tx.receiver_count() > 0 {
                    let _ = self.tx.send(Arc::new(SharedTick::new(normalized)));
                }
                if self.quote_tx.receiver_count() > 0 {
                    let _ = self
                        .quote_tx
                        .send(Arc::new(SharedQuote::new(quote_envelope)));
                }
            }
            DataEvent::FundingRate(t) => {
                let shard = shard_for_key(&t.symbol, self.funding_tx.len());
                let _ = self.funding_tx[shard].send(shared.clone());
                self.snapshots.upsert_funding(t);
            }
            DataEvent::OpenInterest(t) => {
                let shard = shard_for_key(&t.symbol, self.open_interest_tx.len());
                let _ = self.open_interest_tx[shard].send(shared.clone());
                self.snapshots.upsert_open_interest(t);
            }
            DataEvent::Trade(t) => {
                let shard = shard_for_key(&t.symbol, self.trade_tx.len());
                let _ = self.trade_tx[shard].send(shared.clone());
                self.snapshots.upsert_trade(t);
            }
            DataEvent::Liquidation(t) => {
                let shard = shard_for_key(&t.symbol, self.liquidation_tx.len());
                let _ = self.liquidation_tx[shard].send(shared.clone());
                self.snapshots.upsert_liquidation(t);
            }
            DataEvent::OrderBook(t) => {
                let shard = shard_for_key(&t.symbol, self.order_book_tx.len());
                let _ = self.order_book_tx[shard].send(shared.clone());
                self.snapshots.upsert_order_book(t);
            }
            DataEvent::ExternalSignal(t) => {
                let shard = shard_for_key(
                    t.symbol.as_deref().unwrap_or(t.source),
                    self.external_signal_tx.len(),
                );
                let _ = self.external_signal_tx[shard].send(shared.clone());
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

    pub async fn quote_snapshots_matching(
        &self,
        predicate: impl Fn(&DataEnvelope<QuotePayload>) -> bool,
    ) -> Vec<DataEnvelope<QuotePayload>> {
        self.snapshots.quote_snapshots_matching(predicate).await
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

    pub async fn order_book_snapshots_matching(
        &self,
        predicate: impl Fn(&OrderBookTick) -> bool,
    ) -> Vec<OrderBookTick> {
        self.snapshots
            .order_book_snapshots_matching(predicate)
            .await
    }

    pub async fn external_signal_snapshot_all(&self) -> Vec<ExternalSignalTick> {
        self.snapshots.external_signal_snapshot_all().await
    }
}

fn sharded_senders<T: Clone>(capacity: usize, shards: usize) -> Vec<broadcast::Sender<T>> {
    (0..shards)
        .map(|_| {
            let (tx, _) = broadcast::channel(capacity);
            tx
        })
        .collect()
}

fn subscribe_sharded(senders: &[broadcast::Sender<Arc<SharedEvent>>]) -> EventSubscription {
    let (tx, rx) = mpsc::channel(senders.len().saturating_mul(1024).max(1024));
    let mut tasks = Vec::with_capacity(senders.len());
    for sender in senders {
        let mut shard_rx = sender.subscribe();
        let tx = tx.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                match shard_rx.recv().await {
                    Ok(event) => {
                        if tx.send(Ok(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        if tx
                            .send(Err(broadcast::error::RecvError::Lagged(skipped)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
    }
    drop(tx);
    EventSubscription { rx, tasks }
}

fn shard_for_event(event: &DataEvent, shards: usize) -> usize {
    match event {
        DataEvent::Tick(t) => shard_for_key(&t.symbol, shards),
        DataEvent::FundingRate(t) => shard_for_key(&t.symbol, shards),
        DataEvent::OpenInterest(t) => shard_for_key(&t.symbol, shards),
        DataEvent::Trade(t) => shard_for_key(&t.symbol, shards),
        DataEvent::Liquidation(t) => shard_for_key(&t.symbol, shards),
        DataEvent::OrderBook(t) => shard_for_key(&t.symbol, shards),
        DataEvent::ExternalSignal(t) => {
            shard_for_key(t.symbol.as_deref().unwrap_or(t.source), shards)
        }
        DataEvent::Heartbeat { exchange, .. } => shard_for_key(exchange, shards),
    }
}

fn shard_for_key(key: &str, shards: usize) -> usize {
    if shards <= 1 {
        return 0;
    }
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % shards
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
        let bus = EventBus::new_sharded(16, 1000, 1);
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
        let bus = EventBus::new_sharded(16, 1000, 1);
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
        assert_eq!(bus.order_book_snapshots_matching(|_| true).await.len(), 1);
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
        let bus = EventBus::new_sharded(16, 1000, 1);
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

    #[tokio::test]
    async fn sharded_domain_subscription_fans_in_all_shards() {
        let bus = EventBus::new_sharded(16, 1000, 4);
        let mut rx = bus.subscribe_domain(EventDomain::Trade);
        let ts_ms = now_ms();

        for symbol in ["BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT"] {
            bus.publish_from_event(&DataEvent::Trade(TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: symbol.into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms,
            }));
        }

        let mut received = Vec::new();
        for _ in 0..4 {
            let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
                .await
                .expect("sharded subscription should not stall")
                .expect("trade event should publish");
            if let DataEvent::Trade(tick) = event.event.as_ref() {
                received.push(tick.symbol.to_string());
            }
        }
        received.sort();
        assert_eq!(received, ["BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT"]);
    }
}
