use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{RwLock, broadcast};

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::{QuotePayload, envelope_from_tick};
use crate::types::{DataEvent, MarketKind, now_ms};

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
            stale_ttl_ms,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedTick> {
        self.tx.subscribe()
    }

    pub async fn publish_from_event(&self, event: &DataEvent) {
        if let DataEvent::Tick(t) = event {
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
            {
                let mut guard = self.snapshots.write().await;
                guard.insert(key, normalized.clone());
            }
            let _ = self.tx.send(normalized);

            let quote_envelope = envelope_from_tick(self.with_current_stale(t));
            let quote_key = quote_snapshot_key(&quote_envelope);
            {
                let mut guard = self.quote_snapshots.write().await;
                guard.insert(quote_key, quote_envelope.clone());
            }
            let _ = self.quote_tx.send(quote_envelope);
        }
    }

    fn with_current_stale(&self, t: &crate::types::MarketTick) -> NormalizedTick {
        let now = now_ms();
        let latency = now.saturating_sub(t.ts_ms);
        NormalizedTick {
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

fn quote_snapshot_key(envelope: &DataEnvelope<QuotePayload>) -> String {
    format!(
        "{}:{}",
        envelope.source_ref.source, envelope.instrument_ref.instrument_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DataEvent, MarketKind, MarketTick};

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
}
