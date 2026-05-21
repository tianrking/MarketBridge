use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::{QuotePayload, envelope_from_tick};
use crate::types::{
    ExternalSignalTick, FundingRateTick, LiquidationTick, MarketKind, MarketTick, OpenInterestTick,
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
pub struct EventSnapshotStore {
    snapshots: SnapshotMap<NormalizedTick>,
    quote_snapshots: SnapshotMap<DataEnvelope<QuotePayload>>,
    funding_snapshots: SnapshotMap<FundingRateTick>,
    open_interest_snapshots: SnapshotMap<OpenInterestTick>,
    trade_snapshots: SnapshotMap<TradeTick>,
    liquidation_snapshots: SnapshotMap<LiquidationTick>,
    order_book_snapshots: SnapshotMap<OrderBookTick>,
    external_signal_snapshots: SnapshotMap<ExternalSignalTick>,
}

impl EventSnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: SnapshotMap::new(),
            quote_snapshots: SnapshotMap::new(),
            funding_snapshots: SnapshotMap::new(),
            open_interest_snapshots: SnapshotMap::new(),
            trade_snapshots: SnapshotMap::new(),
            liquidation_snapshots: SnapshotMap::new(),
            order_book_snapshots: SnapshotMap::new(),
            external_signal_snapshots: SnapshotMap::new(),
        }
    }

    pub fn upsert_tick(
        &self,
        tick: &MarketTick,
        stale_ttl_ms: u64,
    ) -> (NormalizedTick, DataEnvelope<QuotePayload>) {
        let now = now_ms();
        let latency = now.saturating_sub(tick.ts_ms);
        let normalized = NormalizedTick {
            version: SCHEMA_VERSION,
            exchange: tick.exchange,
            market: market_to_str(tick.market),
            symbol: tick.symbol.to_string(),
            bid: tick.bid,
            ask: tick.ask,
            mark: tick.mark,
            funding: tick.funding_rate,
            ts: tick.ts_ms,
            source_latency_ms: latency,
            stale: latency > stale_ttl_ms,
        };
        let quote_envelope = envelope_from_tick(normalized.clone());

        self.snapshots.upsert(
            snapshot_key(normalized.exchange, normalized.market, &normalized.symbol),
            normalized.clone(),
        );
        self.quote_snapshots
            .upsert(quote_snapshot_key(&quote_envelope), quote_envelope.clone());

        (normalized, quote_envelope)
    }

    pub fn upsert_funding(&self, tick: &FundingRateTick) {
        self.funding_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub fn upsert_open_interest(&self, tick: &OpenInterestTick) {
        self.open_interest_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub fn upsert_trade(&self, tick: &TradeTick) {
        self.trade_snapshots.upsert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
    }

    pub fn upsert_liquidation(&self, tick: &LiquidationTick) {
        self.liquidation_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub fn upsert_order_book(&self, tick: &OrderBookTick) {
        self.order_book_snapshots.upsert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
    }

    pub fn upsert_external_signal(&self, tick: &ExternalSignalTick) {
        self.external_signal_snapshots
            .upsert(external_signal_key(tick), tick.clone());
    }

    pub async fn snapshot_by_symbol(&self, symbol: &str) -> Vec<NormalizedTick> {
        let needle = symbol.to_ascii_uppercase();
        self.snapshots
            .values_matching(|tick| tick.symbol.eq_ignore_ascii_case(&needle))
    }

    pub async fn snapshot_all(&self) -> Vec<NormalizedTick> {
        self.snapshots.values()
    }

    pub async fn quote_snapshot_all(&self) -> Vec<DataEnvelope<QuotePayload>> {
        self.quote_snapshots.values()
    }

    pub async fn funding_snapshot_all(&self) -> Vec<FundingRateTick> {
        self.funding_snapshots.values()
    }

    pub async fn open_interest_snapshot_all(&self) -> Vec<OpenInterestTick> {
        self.open_interest_snapshots.values()
    }

    pub async fn trade_snapshot_all(&self) -> Vec<TradeTick> {
        self.trade_snapshots.values()
    }

    pub async fn liquidation_snapshot_all(&self) -> Vec<LiquidationTick> {
        self.liquidation_snapshots.values()
    }

    pub async fn order_book_snapshot_all(&self) -> Vec<OrderBookTick> {
        self.order_book_snapshots.values()
    }

    pub async fn external_signal_snapshot_all(&self) -> Vec<ExternalSignalTick> {
        self.external_signal_snapshots.values()
    }
}

struct SnapshotMap<T> {
    inner: Arc<DashMap<String, T>>,
}

impl<T> Clone for SnapshotMap<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Clone> SnapshotMap<T> {
    fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn upsert(&self, key: String, value: T) {
        self.inner.insert(key, value);
    }

    fn values(&self) -> Vec<T> {
        self.inner
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    fn values_matching(&self, predicate: impl Fn(&T) -> bool) -> Vec<T> {
        self.inner
            .iter()
            .filter_map(|entry| {
                let value = entry.value();
                predicate(value).then(|| value.clone())
            })
            .collect()
    }
}

fn market_to_str(market: MarketKind) -> &'static str {
    match market {
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

fn external_signal_key(tick: &ExternalSignalTick) -> String {
    format!(
        "{}:{}:{}:{}",
        tick.source,
        tick.category,
        tick.symbol.as_deref().unwrap_or("*"),
        tick.metric
    )
}
