use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

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
    snapshots: Arc<RwLock<HashMap<String, NormalizedTick>>>,
    quote_snapshots: Arc<RwLock<HashMap<String, DataEnvelope<QuotePayload>>>>,
    funding_snapshots: Arc<RwLock<HashMap<String, FundingRateTick>>>,
    open_interest_snapshots: Arc<RwLock<HashMap<String, OpenInterestTick>>>,
    trade_snapshots: Arc<RwLock<HashMap<String, TradeTick>>>,
    liquidation_snapshots: Arc<RwLock<HashMap<String, LiquidationTick>>>,
    order_book_snapshots: Arc<RwLock<HashMap<String, OrderBookTick>>>,
    external_signal_snapshots: Arc<RwLock<HashMap<String, ExternalSignalTick>>>,
}

impl EventSnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            quote_snapshots: Arc::new(RwLock::new(HashMap::new())),
            funding_snapshots: Arc::new(RwLock::new(HashMap::new())),
            open_interest_snapshots: Arc::new(RwLock::new(HashMap::new())),
            trade_snapshots: Arc::new(RwLock::new(HashMap::new())),
            liquidation_snapshots: Arc::new(RwLock::new(HashMap::new())),
            order_book_snapshots: Arc::new(RwLock::new(HashMap::new())),
            external_signal_snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn upsert_tick(
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

        self.snapshots.write().await.insert(
            snapshot_key(normalized.exchange, normalized.market, &normalized.symbol),
            normalized.clone(),
        );
        self.quote_snapshots
            .write()
            .await
            .insert(quote_snapshot_key(&quote_envelope), quote_envelope.clone());

        (normalized, quote_envelope)
    }

    pub async fn upsert_funding(&self, tick: &FundingRateTick) {
        self.funding_snapshots
            .write()
            .await
            .insert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub async fn upsert_open_interest(&self, tick: &OpenInterestTick) {
        self.open_interest_snapshots
            .write()
            .await
            .insert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub async fn upsert_trade(&self, tick: &TradeTick) {
        self.trade_snapshots.write().await.insert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
    }

    pub async fn upsert_liquidation(&self, tick: &LiquidationTick) {
        self.liquidation_snapshots
            .write()
            .await
            .insert(perp_key(tick.exchange, &tick.symbol), tick.clone());
    }

    pub async fn upsert_order_book(&self, tick: &OrderBookTick) {
        self.order_book_snapshots.write().await.insert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
    }

    pub async fn upsert_external_signal(&self, tick: &ExternalSignalTick) {
        self.external_signal_snapshots
            .write()
            .await
            .insert(external_signal_key(tick), tick.clone());
    }

    pub async fn snapshot_by_symbol(&self, symbol: &str) -> Vec<NormalizedTick> {
        let needle = symbol.to_ascii_uppercase();
        self.snapshots
            .read()
            .await
            .values()
            .filter(|t| t.symbol.eq_ignore_ascii_case(&needle))
            .cloned()
            .collect()
    }

    pub async fn snapshot_all(&self) -> Vec<NormalizedTick> {
        self.snapshots.read().await.values().cloned().collect()
    }

    pub async fn quote_snapshot_all(&self) -> Vec<DataEnvelope<QuotePayload>> {
        self.quote_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn funding_snapshot_all(&self) -> Vec<FundingRateTick> {
        self.funding_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn open_interest_snapshot_all(&self) -> Vec<OpenInterestTick> {
        self.open_interest_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn trade_snapshot_all(&self) -> Vec<TradeTick> {
        self.trade_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn liquidation_snapshot_all(&self) -> Vec<LiquidationTick> {
        self.liquidation_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn order_book_snapshot_all(&self) -> Vec<OrderBookTick> {
        self.order_book_snapshots
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn external_signal_snapshot_all(&self) -> Vec<ExternalSignalTick> {
        self.external_signal_snapshots
            .read()
            .await
            .values()
            .cloned()
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
