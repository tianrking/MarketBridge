use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;

use crate::core::schema::DataEnvelope;
use crate::domains::market::quote::{QuotePayload, envelope_from_tick};
use crate::types::{
    ExternalSignalTick, FundingRateTick, LiquidationTick, MarketKind, MarketTick, OpenInterestTick,
    OrderBookTick, TradeTick, now_ms, timestamp_is_fresh,
};

pub const SCHEMA_VERSION: &str = "v1";
const SNAPSHOT_MIN_RETENTION_MS: u64 = 300_000;
const SNAPSHOT_MAX_RETENTION_MS: u64 = 3_600_000;
const SNAPSHOT_RETENTION_MULTIPLIER: u64 = 120;
const MAX_SNAPSHOT_KEYS_PER_DOMAIN: usize = 100_000;

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
    stale_ttl_ms: u64,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
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
    pub fn new(stale_ttl_ms: u64) -> Self {
        Self {
            stale_ttl_ms,
            clock: Arc::new(now_ms),
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
        let now = (self.clock)();
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
            stale: !timestamp_is_fresh(tick.ts_ms, now, stale_ttl_ms),
        };
        let mut quote_envelope = envelope_from_tick(normalized.clone());
        quote_envelope.freshness.ts_received = now;

        self.snapshots.upsert(
            snapshot_key(normalized.exchange, normalized.market, &normalized.symbol),
            normalized.clone(),
        );
        self.snapshots.prune_by_ts(
            now,
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
        self.quote_snapshots
            .upsert(quote_snapshot_key(&quote_envelope), quote_envelope.clone());
        self.quote_snapshots.prune_by_ts(
            now,
            snapshot_retention_ms(stale_ttl_ms),
            |envelope| envelope.freshness.ts_source,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );

        (normalized, quote_envelope)
    }

    pub fn upsert_funding(&self, tick: &FundingRateTick, stale_ttl_ms: u64) {
        self.funding_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
        self.funding_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub fn upsert_open_interest(&self, tick: &OpenInterestTick, stale_ttl_ms: u64) {
        self.open_interest_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
        self.open_interest_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub fn upsert_trade(&self, tick: &TradeTick, stale_ttl_ms: u64) {
        self.trade_snapshots.upsert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
        self.trade_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub fn upsert_liquidation(&self, tick: &LiquidationTick, stale_ttl_ms: u64) {
        self.liquidation_snapshots
            .upsert(perp_key(tick.exchange, &tick.symbol), tick.clone());
        self.liquidation_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub fn upsert_order_book(&self, tick: &OrderBookTick, stale_ttl_ms: u64) {
        self.order_book_snapshots.upsert(
            market_key(tick.exchange, tick.market, &tick.symbol),
            tick.clone(),
        );
        self.order_book_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub fn upsert_external_signal(&self, tick: &ExternalSignalTick, stale_ttl_ms: u64) {
        self.external_signal_snapshots
            .upsert(external_signal_key(tick), tick.clone());
        self.external_signal_snapshots.prune_by_ts(
            now_ms(),
            snapshot_retention_ms(stale_ttl_ms),
            |tick| tick.ts_ms,
            MAX_SNAPSHOT_KEYS_PER_DOMAIN,
        );
    }

    pub async fn snapshot_by_symbol(&self, symbol: &str) -> Vec<NormalizedTick> {
        let needle = symbol.to_ascii_uppercase();
        self.snapshot_all()
            .await
            .into_iter()
            .filter(|tick| tick.symbol.eq_ignore_ascii_case(&needle))
            .collect()
    }

    pub async fn snapshot_all(&self) -> Vec<NormalizedTick> {
        let now = (self.clock)();
        self.snapshots
            .values()
            .into_iter()
            .map(|mut tick| {
                tick.stale = !timestamp_is_fresh(tick.ts, now, self.stale_ttl_ms);
                tick
            })
            .collect()
    }

    pub async fn quote_snapshot_all(&self) -> Vec<DataEnvelope<QuotePayload>> {
        let now = (self.clock)();
        self.quote_snapshots
            .values()
            .into_iter()
            .map(|mut quote| {
                quote.freshness.stale =
                    !timestamp_is_fresh(quote.freshness.ts_source, now, self.stale_ttl_ms);
                quote
            })
            .collect()
    }

    pub async fn quote_snapshots_matching(
        &self,
        predicate: impl Fn(&DataEnvelope<QuotePayload>) -> bool,
    ) -> Vec<DataEnvelope<QuotePayload>> {
        self.quote_snapshot_all()
            .await
            .into_iter()
            .filter(predicate)
            .collect()
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

    pub async fn order_book_snapshots_matching(
        &self,
        predicate: impl Fn(&OrderBookTick) -> bool,
    ) -> Vec<OrderBookTick> {
        self.order_book_snapshots.values_matching(predicate)
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

    fn prune_by_ts(
        &self,
        now: u64,
        retention_ms: u64,
        ts_of: impl Fn(&T) -> u64 + Copy,
        max_entries: usize,
    ) {
        self.inner
            .retain(|_, value| now.saturating_sub(ts_of(value)) <= retention_ms);
        self.enforce_max_entries(max_entries, ts_of);
    }

    fn enforce_max_entries(&self, max_entries: usize, ts_of: impl Fn(&T) -> u64) {
        if max_entries == 0 || self.inner.len() <= max_entries {
            return;
        }
        let mut rows = self
            .inner
            .iter()
            .map(|entry| (entry.key().clone(), ts_of(entry.value())))
            .collect::<Vec<_>>();
        rows.sort_by_key(|(_, ts)| *ts);
        for (key, _) in rows
            .into_iter()
            .take(self.inner.len().saturating_sub(max_entries))
        {
            self.inner.remove(&key);
        }
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

fn snapshot_retention_ms(stale_ttl_ms: u64) -> u64 {
    stale_ttl_ms
        .saturating_mul(SNAPSHOT_RETENTION_MULTIPLIER)
        .clamp(SNAPSHOT_MIN_RETENTION_MS, SNAPSHOT_MAX_RETENTION_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn silent_snapshot_expires_at_read_without_rewriting_receipt_time() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let clock = Arc::new(AtomicU64::new(10_010));
        let mut store = EventSnapshotStore::new(1_000);
        let clock_ref = clock.clone();
        store.clock = Arc::new(move || clock_ref.load(Ordering::Relaxed));
        store.upsert_tick(
            &MarketTick {
                exchange: "test",
                market: MarketKind::Spot,
                symbol: "BTCUSDT".into(),
                bid: 99.0,
                ask: 100.0,
                mark: None,
                funding_rate: None,
                ts_ms: 10_000,
            },
            1_000,
        );
        assert!(!store.snapshot_all().await[0].stale);
        clock.store(11_001, Ordering::Relaxed);
        assert!(store.snapshot_by_symbol("btcusdt").await[0].stale);
        let quotes = store.quote_snapshots_matching(|q| q.freshness.stale).await;
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].freshness.ts_received, 10_010);
        assert_eq!(quotes[0].freshness.latency_ms, 10);
    }

    #[test]
    fn snapshot_retention_uses_longer_window_than_stale_ttl() {
        assert_eq!(snapshot_retention_ms(1_500), SNAPSHOT_MIN_RETENTION_MS);
        assert_eq!(snapshot_retention_ms(60_000), SNAPSHOT_MAX_RETENTION_MS);
    }

    #[test]
    fn snapshot_map_prunes_old_entries_and_caps_newest() {
        let map = SnapshotMap::new();
        map.upsert("old".to_string(), 100_u64);
        map.upsert("new-a".to_string(), 1_000_u64);
        map.upsert("new-b".to_string(), 1_100_u64);

        map.prune_by_ts(1_100, 500, |ts| *ts, 1);
        let values = map.values();

        assert_eq!(values, vec![1_100]);
    }
}
