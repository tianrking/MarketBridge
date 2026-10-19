use std::collections::HashSet;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::config::runtime::MemoryLimits;
use crate::data_quality::{RetentionQuality, SubscriptionQuality};
use crate::event_bus::{EventBus, EventDomain};
use crate::types::{DataEvent, TradeSide, TradeTick};

const DEFAULT_WINDOWS_MS: &[u64] = &[60_000, 300_000, 900_000];
const MAX_BUCKETS_PER_KEY: usize = 500;
const MAX_RAW_TRADES_PER_KEY: usize = 20_000;
const RAW_TRADE_RETENTION_MS: u64 = 30 * 60_000;
const BUCKET_RETENTION_MS: u64 = 24 * 60 * 60_000;
const GLOBAL_PRUNE_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone, Serialize)]
pub struct OrderFlowBucket {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub window_ms: u64,
    pub bucket_start_ms: u64,
    pub bucket_end_ms: u64,
    pub buy_qty: f64,
    pub sell_qty: f64,
    pub buy_notional: f64,
    pub sell_notional: f64,
    pub trade_count: u64,
    pub buy_trade_count: u64,
    pub sell_trade_count: u64,
    pub large_trade_count: u64,
    pub delta_qty: f64,
    pub delta_notional: f64,
    pub cumulative_delta_qty: f64,
    pub cumulative_delta_notional: f64,
    pub aggressive_buy_ratio: Option<f64>,
    pub aggressive_sell_ratio: Option<f64>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct OrderFlowQuery {
    pub exchange: Option<String>,
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub window_ms: Option<u64>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct FootprintQuery {
    pub exchange: Option<String>,
    pub market: Option<String>,
    pub symbol: Option<String>,
    pub interval_ms: u64,
    pub scale: f64,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub imbalance_ratio: f64,
    pub imbalance_volume: f64,
    pub stacked_imbalance_range: usize,
    pub include_trades: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FootprintCandle {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub interval_ms: u64,
    pub bucket_start_ms: u64,
    pub bucket_end_ms: u64,
    pub scale: f64,
    pub bid_qty: f64,
    pub ask_qty: f64,
    pub bid_notional: f64,
    pub ask_notional: f64,
    pub delta_qty: f64,
    pub delta_notional: f64,
    pub min_delta_qty: f64,
    pub max_delta_qty: f64,
    pub min_delta_notional: f64,
    pub max_delta_notional: f64,
    pub aggressive_buy_ratio: Option<f64>,
    pub aggressive_sell_ratio: Option<f64>,
    pub total_trades: u64,
    pub stacked_imbalances_bid: Vec<f64>,
    pub stacked_imbalances_ask: Vec<f64>,
    pub levels: Vec<FootprintLevel>,
    pub trades: Vec<FootprintTrade>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FootprintLevel {
    pub price: f64,
    pub bid_qty: f64,
    pub ask_qty: f64,
    pub bid_notional: f64,
    pub ask_notional: f64,
    pub bid_trades: u64,
    pub ask_trades: u64,
    pub delta_qty: f64,
    pub delta_notional: f64,
    pub total_qty: f64,
    pub total_notional: f64,
    pub total_trades: u64,
    pub bid_imbalance: bool,
    pub ask_imbalance: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FootprintTrade {
    pub ts_ms: u64,
    pub price: f64,
    pub qty: f64,
    pub side: String,
    pub trade_id: Option<String>,
    pub notional: f64,
}

#[derive(Clone)]
pub struct OrderFlowStore {
    inner: Arc<RwLock<OrderFlowState>>,
    large_trade_notional_usdt: f64,
    limits: MemoryLimits,
}

#[derive(Default)]
struct OrderFlowState {
    subscription_quality: SubscriptionQuality,
    raw_retention_quality: RetentionQuality,
    bucket_retention_quality: RetentionQuality,
    raw_records: usize,
    raw_bytes: usize,
    watermark_ms: u64,
    last_prune_ms: Option<u64>,
    buckets: HashMap<String, OrderFlowBucket>,
    bucket_counts: HashMap<String, usize>,
    cumulative_qty: HashMap<String, f64>,
    cumulative_notional: HashMap<String, f64>,
    raw_trades: HashMap<String, VecDeque<StoredTrade>>,
}

#[derive(Debug, Clone)]
struct StoredTrade {
    exchange: String,
    market: String,
    symbol: String,
    ts_ms: u64,
    price: f64,
    qty: f64,
    side: TradeSide,
    trade_id: Option<String>,
    notional: f64,
}

impl OrderFlowStore {
    #[cfg(test)]
    pub fn new(large_trade_notional_usdt: f64) -> Self {
        Self::with_limits(large_trade_notional_usdt, MemoryLimits::default())
    }

    pub fn with_limits(large_trade_notional_usdt: f64, limits: MemoryLimits) -> Self {
        Self {
            inner: Arc::new(RwLock::new(OrderFlowState::default())),
            large_trade_notional_usdt,
            limits,
        }
    }

    pub async fn retention_status(&self, footprint: bool) -> serde_json::Value {
        let guard = self.inner.read().await;
        serde_json::json!({
            "quality": if footprint { &guard.raw_retention_quality } else { &guard.bucket_retention_quality },
            "raw_records": guard.raw_records, "raw_accounted_bytes": guard.raw_bytes,
            "raw_keys": guard.raw_trades.len(), "bucket_rows": guard.buckets.len(),
            "accounting": "owned raw deque capacity and row strings; not process RSS",
            "limits": self.limits,
        })
    }

    pub async fn subscription_quality(&self) -> SubscriptionQuality {
        self.inner.read().await.subscription_quality.clone()
    }

    async fn record_subscriber_gap(&self, skipped: u64) {
        self.inner
            .write()
            .await
            .subscription_quality
            .record_gap(skipped, crate::types::now_ms());
    }

    pub async fn update_trade(&self, trade: &TradeTick) {
        let market = market_label(trade.market).to_string();
        let symbol = trade.symbol.to_ascii_uppercase();
        let notional = trade.price * trade.qty;
        if !notional.is_finite() || notional <= 0.0 {
            return;
        }
        let mut guard = self.inner.write().await;
        let raw_key = format!("{}:{}:{}", trade.exchange, market, symbol);
        guard.watermark_ms = guard.watermark_ms.max(trade.ts_ms);
        let watermark = guard.watermark_ms;
        if watermark.saturating_sub(trade.ts_ms) <= RAW_TRADE_RETENTION_MS {
            let stored = StoredTrade {
                exchange: trade.exchange.to_string(),
                market: market.clone(),
                symbol: symbol.clone(),
                ts_ms: trade.ts_ms,
                price: trade.price,
                qty: trade.qty,
                side: trade.side,
                trade_id: trade.trade_id.as_ref().map(|id| id.to_string()),
                notional,
            };
            let dynamic_bytes = stored.dynamic_bytes();
            let raw = guard.raw_trades.entry(raw_key).or_default();
            let old_capacity = raw.capacity();
            raw.push_back(stored);
            let mut removed_bytes = 0;
            let mut removed_rows = 0;
            while raw.len() > MAX_RAW_TRADES_PER_KEY {
                removed_bytes += raw.pop_front().unwrap().dynamic_bytes();
                removed_rows += 1;
            }
            let capacity_delta =
                (raw.capacity() - old_capacity) * std::mem::size_of::<StoredTrade>();
            guard.raw_records = guard.raw_records + 1 - removed_rows;
            guard.raw_bytes = guard.raw_bytes + capacity_delta + dynamic_bytes - removed_bytes;
            guard.raw_retention_quality.record_eviction(removed_rows);
        }

        for window_ms in DEFAULT_WINDOWS_MS {
            let bucket_start_ms = trade.ts_ms / window_ms * window_ms;
            let bucket_end_ms = bucket_start_ms.saturating_add(*window_ms - 1);
            if watermark.saturating_sub(bucket_end_ms) > BUCKET_RETENTION_MS {
                continue;
            }
            let cvd_key = format!("{}:{}:{}:{}", trade.exchange, market, symbol, window_ms);
            let key = format!("{cvd_key}:{bucket_start_ms}");
            let delta_sign = match trade.side {
                TradeSide::Buy => 1.0,
                TradeSide::Sell => -1.0,
                TradeSide::Unknown => 0.0,
            };
            let cumulative_qty = guard.cumulative_qty.entry(cvd_key.clone()).or_default();
            *cumulative_qty += delta_sign * trade.qty;
            let cumulative_delta_qty = *cumulative_qty;
            let cumulative_notional = guard
                .cumulative_notional
                .entry(cvd_key.clone())
                .or_default();
            *cumulative_notional += delta_sign * notional;
            let cumulative_delta_notional = *cumulative_notional;

            let was_new = !guard.buckets.contains_key(&key);
            let bucket = guard.buckets.entry(key).or_insert_with(|| OrderFlowBucket {
                exchange: trade.exchange.to_string(),
                market: market.clone(),
                symbol: symbol.clone(),
                window_ms: *window_ms,
                bucket_start_ms,
                bucket_end_ms,
                buy_qty: 0.0,
                sell_qty: 0.0,
                buy_notional: 0.0,
                sell_notional: 0.0,
                trade_count: 0,
                buy_trade_count: 0,
                sell_trade_count: 0,
                large_trade_count: 0,
                delta_qty: 0.0,
                delta_notional: 0.0,
                cumulative_delta_qty,
                cumulative_delta_notional,
                aggressive_buy_ratio: None,
                aggressive_sell_ratio: None,
                updated_at_ms: crate::types::now_ms(),
            });
            match trade.side {
                TradeSide::Buy => {
                    bucket.buy_qty += trade.qty;
                    bucket.buy_notional += notional;
                    bucket.buy_trade_count += 1;
                }
                TradeSide::Sell => {
                    bucket.sell_qty += trade.qty;
                    bucket.sell_notional += notional;
                    bucket.sell_trade_count += 1;
                }
                TradeSide::Unknown => {}
            }
            bucket.trade_count += 1;
            if notional >= self.large_trade_notional_usdt {
                bucket.large_trade_count += 1;
            }
            bucket.delta_qty = bucket.buy_qty - bucket.sell_qty;
            bucket.delta_notional = bucket.buy_notional - bucket.sell_notional;
            bucket.cumulative_delta_qty = cumulative_delta_qty;
            bucket.cumulative_delta_notional = cumulative_delta_notional;
            let total_known_trades = bucket.buy_trade_count + bucket.sell_trade_count;
            if total_known_trades > 0 {
                bucket.aggressive_buy_ratio =
                    Some(bucket.buy_trade_count as f64 / total_known_trades as f64);
                bucket.aggressive_sell_ratio =
                    Some(bucket.sell_trade_count as f64 / total_known_trades as f64);
            }
            bucket.updated_at_ms = crate::types::now_ms();
            if was_new {
                *guard.bucket_counts.entry(cvd_key.clone()).or_default() += 1;
                if guard.bucket_counts[&cvd_key] > MAX_BUCKETS_PER_KEY {
                    let prefix = format!("{cvd_key}:");
                    let oldest = guard
                        .buckets
                        .iter()
                        .filter(|(key, _)| key.starts_with(&prefix))
                        .min_by_key(|(key, row)| (row.bucket_start_ms, *key))
                        .map(|(key, _)| key.clone())
                        .unwrap();
                    guard.buckets.remove(&oldest);
                    *guard.bucket_counts.get_mut(&cvd_key).unwrap() -= 1;
                    guard.bucket_retention_quality.record_eviction(1);
                }
            }
        }
        if guard
            .last_prune_ms
            .is_none_or(|last| watermark.saturating_sub(last) >= GLOBAL_PRUNE_INTERVAL_MS)
        {
            prune_order_flow_state(&mut guard, watermark);
            guard.last_prune_ms = Some(watermark);
        }
        enforce_memory_budget(&mut guard, &self.limits);
    }

    pub async fn query(&self, q: OrderFlowQuery) -> Vec<OrderFlowBucket> {
        let guard = self.inner.read().await;
        let mut rows = guard
            .buckets
            .values()
            .filter(|row| {
                q.exchange
                    .as_ref()
                    .is_none_or(|value| row.exchange.eq_ignore_ascii_case(value))
            })
            .filter(|row| {
                q.market
                    .as_ref()
                    .is_none_or(|value| row.market.eq_ignore_ascii_case(value))
            })
            .filter(|row| {
                q.symbol
                    .as_ref()
                    .is_none_or(|value| row.symbol.eq_ignore_ascii_case(value))
            })
            .filter(|row| q.window_ms.is_none_or(|value| row.window_ms == value))
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            b.bucket_start_ms
                .cmp(&a.bucket_start_ms)
                .then(a.exchange.cmp(&b.exchange))
                .then(a.symbol.cmp(&b.symbol))
        });
        rows.truncate(q.limit.clamp(1, 5000));
        rows
    }

    pub async fn query_footprint(&self, q: FootprintQuery) -> Vec<FootprintCandle> {
        if q.interval_ms == 0 || !q.scale.is_finite() || q.scale <= 0.0 {
            return Vec::new();
        }

        let guard = self.inner.read().await;
        let mut grouped = BTreeMap::<(String, String, String, u64), FootprintBuilder>::new();
        for trades in guard.raw_trades.values() {
            for trade in trades {
                if !matches_footprint_query(trade, &q) {
                    continue;
                }
                let bucket_start_ms = trade.ts_ms / q.interval_ms * q.interval_ms;
                let key = (
                    trade.exchange.clone(),
                    trade.market.clone(),
                    trade.symbol.clone(),
                    bucket_start_ms,
                );
                grouped
                    .entry(key)
                    .or_insert_with(|| FootprintBuilder::new(q.interval_ms, bucket_start_ms))
                    .push(trade, q.scale, q.include_trades);
            }
        }

        let mut rows = grouped
            .into_iter()
            .map(|((exchange, market, symbol, _), builder)| {
                builder.finish(FootprintFinishOptions {
                    exchange,
                    market,
                    symbol,
                    scale: q.scale,
                    imbalance_ratio: q.imbalance_ratio,
                    imbalance_volume: q.imbalance_volume,
                    stacked_imbalance_range: q.stacked_imbalance_range,
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            b.bucket_start_ms
                .cmp(&a.bucket_start_ms)
                .then(a.exchange.cmp(&b.exchange))
                .then(a.symbol.cmp(&b.symbol))
        });
        rows.truncate(q.limit.clamp(1, 1000));
        rows
    }
}

struct FootprintBuilder {
    interval_ms: u64,
    bucket_start_ms: u64,
    bid_qty: f64,
    ask_qty: f64,
    bid_notional: f64,
    ask_notional: f64,
    running_delta_qty: f64,
    running_delta_notional: f64,
    min_delta_qty: f64,
    max_delta_qty: f64,
    min_delta_notional: f64,
    max_delta_notional: f64,
    total_trades: u64,
    levels: HashMap<i64, FootprintLevelBuilder>,
    trades: Vec<FootprintTrade>,
}

struct FootprintFinishOptions {
    exchange: String,
    market: String,
    symbol: String,
    scale: f64,
    imbalance_ratio: f64,
    imbalance_volume: f64,
    stacked_imbalance_range: usize,
}

#[derive(Default)]
struct FootprintLevelBuilder {
    bid_qty: f64,
    ask_qty: f64,
    bid_notional: f64,
    ask_notional: f64,
    bid_trades: u64,
    ask_trades: u64,
}

impl FootprintBuilder {
    fn new(interval_ms: u64, bucket_start_ms: u64) -> Self {
        Self {
            interval_ms,
            bucket_start_ms,
            bid_qty: 0.0,
            ask_qty: 0.0,
            bid_notional: 0.0,
            ask_notional: 0.0,
            running_delta_qty: 0.0,
            running_delta_notional: 0.0,
            min_delta_qty: 0.0,
            max_delta_qty: 0.0,
            min_delta_notional: 0.0,
            max_delta_notional: 0.0,
            total_trades: 0,
            levels: HashMap::new(),
            trades: Vec::new(),
        }
    }

    fn push(&mut self, trade: &StoredTrade, scale: f64, include_trade: bool) {
        let price_bin = (trade.price / scale).round() as i64;
        let level = self.levels.entry(price_bin).or_default();
        match trade.side {
            TradeSide::Buy => {
                self.ask_qty += trade.qty;
                self.ask_notional += trade.notional;
                level.ask_qty += trade.qty;
                level.ask_notional += trade.notional;
                level.ask_trades += 1;
                self.running_delta_qty += trade.qty;
                self.running_delta_notional += trade.notional;
            }
            TradeSide::Sell => {
                self.bid_qty += trade.qty;
                self.bid_notional += trade.notional;
                level.bid_qty += trade.qty;
                level.bid_notional += trade.notional;
                level.bid_trades += 1;
                self.running_delta_qty -= trade.qty;
                self.running_delta_notional -= trade.notional;
            }
            TradeSide::Unknown => {}
        }
        self.min_delta_qty = self.min_delta_qty.min(self.running_delta_qty);
        self.max_delta_qty = self.max_delta_qty.max(self.running_delta_qty);
        self.min_delta_notional = self.min_delta_notional.min(self.running_delta_notional);
        self.max_delta_notional = self.max_delta_notional.max(self.running_delta_notional);
        self.total_trades += 1;
        if include_trade {
            self.trades.push(FootprintTrade {
                ts_ms: trade.ts_ms,
                price: trade.price,
                qty: trade.qty,
                side: trade_side_label(trade.side).to_string(),
                trade_id: trade.trade_id.clone(),
                notional: trade.notional,
            });
        }
    }

    fn finish(self, options: FootprintFinishOptions) -> FootprintCandle {
        let mut rows = self
            .levels
            .into_iter()
            .map(|(bin, level)| {
                let total_qty = level.bid_qty + level.ask_qty;
                let total_notional = level.bid_notional + level.ask_notional;
                FootprintLevel {
                    price: bin as f64 * options.scale,
                    bid_qty: level.bid_qty,
                    ask_qty: level.ask_qty,
                    bid_notional: level.bid_notional,
                    ask_notional: level.ask_notional,
                    bid_trades: level.bid_trades,
                    ask_trades: level.ask_trades,
                    delta_qty: level.ask_qty - level.bid_qty,
                    delta_notional: level.ask_notional - level.bid_notional,
                    total_qty,
                    total_notional,
                    total_trades: level.bid_trades + level.ask_trades,
                    bid_imbalance: false,
                    ask_imbalance: false,
                }
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| a.price.total_cmp(&b.price));
        apply_imbalances(&mut rows, options.imbalance_ratio, options.imbalance_volume);
        let stacked_imbalances_bid = stacked_imbalances(
            &rows,
            |level| level.bid_imbalance,
            options.stacked_imbalance_range,
        );
        let stacked_imbalances_ask = stacked_imbalances(
            &rows,
            |level| level.ask_imbalance,
            options.stacked_imbalance_range,
        );
        let known_trades = rows
            .iter()
            .map(|level| level.bid_trades + level.ask_trades)
            .sum::<u64>();
        let aggressive_buy_ratio = (known_trades > 0).then_some(
            rows.iter().map(|level| level.ask_trades).sum::<u64>() as f64 / known_trades as f64,
        );
        let aggressive_sell_ratio = (known_trades > 0).then_some(
            rows.iter().map(|level| level.bid_trades).sum::<u64>() as f64 / known_trades as f64,
        );

        FootprintCandle {
            exchange: options.exchange,
            market: options.market,
            symbol: options.symbol,
            interval_ms: self.interval_ms,
            bucket_start_ms: self.bucket_start_ms,
            bucket_end_ms: self.bucket_start_ms + self.interval_ms - 1,
            scale: options.scale,
            bid_qty: self.bid_qty,
            ask_qty: self.ask_qty,
            bid_notional: self.bid_notional,
            ask_notional: self.ask_notional,
            delta_qty: self.ask_qty - self.bid_qty,
            delta_notional: self.ask_notional - self.bid_notional,
            min_delta_qty: self.min_delta_qty,
            max_delta_qty: self.max_delta_qty,
            min_delta_notional: self.min_delta_notional,
            max_delta_notional: self.max_delta_notional,
            aggressive_buy_ratio,
            aggressive_sell_ratio,
            total_trades: self.total_trades,
            stacked_imbalances_bid,
            stacked_imbalances_ask,
            levels: rows,
            trades: self.trades,
        }
    }
}

fn matches_footprint_query(trade: &StoredTrade, q: &FootprintQuery) -> bool {
    q.exchange
        .as_ref()
        .is_none_or(|value| trade.exchange.eq_ignore_ascii_case(value))
        && q.market
            .as_ref()
            .is_none_or(|value| trade.market.eq_ignore_ascii_case(value))
        && q.symbol
            .as_ref()
            .is_none_or(|value| trade.symbol.eq_ignore_ascii_case(value))
        && q.start_ms.is_none_or(|value| trade.ts_ms >= value)
        && q.end_ms.is_none_or(|value| trade.ts_ms <= value)
}

fn apply_imbalances(levels: &mut [FootprintLevel], imbalance_ratio: f64, imbalance_volume: f64) {
    if levels.len() < 2 || !imbalance_ratio.is_finite() || imbalance_ratio <= 0.0 {
        return;
    }
    for idx in 0..levels.len() - 1 {
        let next_ask_qty = levels[idx + 1].ask_qty;
        if levels[idx].total_qty >= imbalance_volume
            && next_ask_qty > 0.0
            && levels[idx].bid_qty / next_ask_qty > imbalance_ratio
        {
            levels[idx].bid_imbalance = true;
        }
        if levels[idx].total_qty >= imbalance_volume
            && levels[idx].bid_qty > 0.0
            && next_ask_qty / levels[idx].bid_qty > imbalance_ratio
        {
            levels[idx + 1].ask_imbalance = true;
        }
    }
}

fn stacked_imbalances(
    levels: &[FootprintLevel],
    predicate: impl Fn(&FootprintLevel) -> bool,
    range: usize,
) -> Vec<f64> {
    if range == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut run_start = 0;
    let mut run_len = 0;
    for (idx, level) in levels.iter().enumerate() {
        if predicate(level) {
            if run_len == 0 {
                run_start = idx;
            }
            run_len += 1;
            if run_len >= range {
                out.push(levels[run_start].price);
            }
        } else {
            run_len = 0;
        }
    }
    out
}

pub fn spawn_order_flow_service(
    bus: EventBus,
    store: OrderFlowStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    let mut rx = bus.subscribe_domain(EventDomain::Trade);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                received = rx.recv() => {
                    match received {
                        Ok(event) => {
                            if let DataEvent::Trade(trade) = event.event.as_ref() {
                                store.update_trade(trade).await;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            store.record_subscriber_gap(skipped).await;
                            warn!(skipped, "order-flow subscriber lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    })
}

fn prune_buckets(buckets: &mut HashMap<String, OrderFlowBucket>) {
    let mut grouped: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    for (key, row) in buckets.iter() {
        grouped
            .entry(format!(
                "{}:{}:{}:{}",
                row.exchange, row.market, row.symbol, row.window_ms
            ))
            .or_default()
            .push((key.clone(), row.bucket_start_ms));
    }

    for rows in grouped.values_mut() {
        if rows.len() <= MAX_BUCKETS_PER_KEY {
            continue;
        }
        rows.sort_by_key(|(_, ts)| *ts);
        for (key, _) in rows
            .iter()
            .take(rows.len().saturating_sub(MAX_BUCKETS_PER_KEY))
        {
            buckets.remove(key);
        }
    }
}

fn prune_order_flow_state(state: &mut OrderFlowState, now: u64) {
    state.raw_trades.retain(|_, trades| {
        trades.retain(|trade| now.saturating_sub(trade.ts_ms) <= RAW_TRADE_RETENTION_MS);
        !trades.is_empty()
    });
    recount_raw_memory(state);

    state
        .buckets
        .retain(|_, row| now.saturating_sub(row.bucket_end_ms) <= BUCKET_RETENTION_MS);
    let before = state.buckets.len();
    prune_buckets(&mut state.buckets);
    state
        .bucket_retention_quality
        .record_eviction(before - state.buckets.len());

    recount_bucket_keys(state);
    let active_cvd_keys = state
        .buckets
        .values()
        .map(|row| {
            format!(
                "{}:{}:{}:{}",
                row.exchange, row.market, row.symbol, row.window_ms
            )
        })
        .collect::<HashSet<_>>();
    state
        .cumulative_qty
        .retain(|key, _| active_cvd_keys.contains(key));
    state
        .cumulative_notional
        .retain(|key, _| active_cvd_keys.contains(key));
}

impl StoredTrade {
    fn dynamic_bytes(&self) -> usize {
        self.exchange.capacity()
            + self.market.capacity()
            + self.symbol.capacity()
            + self.trade_id.as_ref().map_or(0, String::capacity)
    }
}

fn history_bytes(rows: &VecDeque<StoredTrade>) -> usize {
    rows.capacity() * std::mem::size_of::<StoredTrade>()
        + rows.iter().map(StoredTrade::dynamic_bytes).sum::<usize>()
}

fn recount_raw_memory(state: &mut OrderFlowState) {
    state.raw_records = state.raw_trades.values().map(VecDeque::len).sum();
    state.raw_bytes = state.raw_trades.values().map(history_bytes).sum();
}

fn recount_bucket_keys(state: &mut OrderFlowState) {
    state.bucket_counts.clear();
    for row in state.buckets.values() {
        *state
            .bucket_counts
            .entry(format!(
                "{}:{}:{}:{}",
                row.exchange, row.market, row.symbol, row.window_ms
            ))
            .or_default() += 1;
    }
}

fn enforce_memory_budget(state: &mut OrderFlowState, limits: &MemoryLimits) {
    while state.raw_records > limits.order_flow_raw_records
        || state.raw_bytes > limits.order_flow_raw_bytes
        || state.raw_trades.len() > limits.order_flow_keys
    {
        let Some(key) = state
            .raw_trades
            .iter()
            .min_by_key(|(key, trades)| (trades.back().map_or(0, |t| t.ts_ms), *key))
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        let removed = state.raw_trades.remove(&key).unwrap();
        state.raw_records -= removed.len();
        state.raw_bytes -= history_bytes(&removed);
        state.raw_retention_quality.record_eviction(removed.len());
    }
    if state.buckets.len() > limits.order_flow_buckets {
        let mut rows = state
            .buckets
            .iter()
            .map(|(key, row)| (row.bucket_start_ms, key.clone()))
            .collect::<Vec<_>>();
        rows.sort();
        let excess = rows.len() - limits.order_flow_buckets;
        for (_, key) in rows.into_iter().take(excess) {
            state.buckets.remove(&key);
        }
        state.bucket_retention_quality.record_eviction(excess);
        recount_bucket_keys(state);
        // CVD maps must not grow independently after key/bucket eviction.
        let active = state
            .buckets
            .values()
            .map(|r| format!("{}:{}:{}:{}", r.exchange, r.market, r.symbol, r.window_ms))
            .collect::<HashSet<_>>();
        state.cumulative_qty.retain(|key, _| active.contains(key));
        state
            .cumulative_notional
            .retain(|key, _| active.contains(key));
    }
}

fn market_label(market: crate::types::MarketKind) -> &'static str {
    match market {
        crate::types::MarketKind::Spot => "spot",
        crate::types::MarketKind::Perp => "perp",
    }
}

fn trade_side_label(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
        TradeSide::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BUCKET_RETENTION_MS, FootprintQuery, MAX_BUCKETS_PER_KEY, OrderFlowQuery, OrderFlowStore,
        RAW_TRADE_RETENTION_MS,
    };
    use crate::types::{MarketKind, TradeSide, TradeTick};

    #[tokio::test]
    async fn order_flow_aggregates_buy_sell_delta() {
        let store = OrderFlowStore::new(100_000.0);
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "BTCUSDT".into(),
                price: 100.0,
                qty: 2.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms: 60_001,
            })
            .await;
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "BTCUSDT".into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Sell,
                trade_id: None,
                ts_ms: 60_002,
            })
            .await;

        let rows = store
            .query(OrderFlowQuery {
                exchange: Some("binance".to_string()),
                market: Some("perp".to_string()),
                symbol: Some("BTCUSDT".to_string()),
                window_ms: Some(60_000),
                limit: 10,
            })
            .await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].buy_qty, 2.0);
        assert_eq!(rows[0].sell_qty, 1.0);
        assert_eq!(rows[0].delta_qty, 1.0);
    }

    #[tokio::test]
    async fn order_flow_prunes_per_symbol_window_not_globally() {
        let store = OrderFlowStore::new(100_000.0);
        for bucket in 0..=MAX_BUCKETS_PER_KEY {
            for symbol in ["BTCUSDT", "ETHUSDT"] {
                store
                    .update_trade(&TradeTick {
                        exchange: "binance",
                        market: MarketKind::Perp,
                        symbol: symbol.into(),
                        price: 100.0,
                        qty: 1.0,
                        side: TradeSide::Buy,
                        trade_id: None,
                        ts_ms: (bucket as u64) * 60_000,
                    })
                    .await;
            }
        }

        for symbol in ["BTCUSDT", "ETHUSDT"] {
            let rows = store
                .query(OrderFlowQuery {
                    exchange: Some("binance".to_string()),
                    market: Some("perp".to_string()),
                    symbol: Some(symbol.to_string()),
                    window_ms: Some(60_000),
                    limit: 1_000,
                })
                .await;
            assert_eq!(rows.len(), MAX_BUCKETS_PER_KEY);
            assert_eq!(
                rows.last().expect("oldest retained").bucket_start_ms,
                60_000
            );
        }
    }

    #[tokio::test]
    async fn order_flow_prunes_stale_raw_trade_keys() {
        let store = OrderFlowStore::new(100_000.0);
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "OLDUSDT".into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms: 1,
            })
            .await;
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "NEWUSDT".into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms: RAW_TRADE_RETENTION_MS + 2,
            })
            .await;

        let guard = store.inner.read().await;
        assert!(!guard.raw_trades.contains_key("binance:perp:OLDUSDT"));
        assert!(guard.raw_trades.contains_key("binance:perp:NEWUSDT"));
    }

    #[tokio::test]
    async fn order_flow_prunes_stale_bucket_keys_and_cvd_state() {
        let store = OrderFlowStore::new(100_000.0);
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "OLDUSDT".into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms: 60_001,
            })
            .await;
        store
            .update_trade(&TradeTick {
                exchange: "binance",
                market: MarketKind::Perp,
                symbol: "NEWUSDT".into(),
                price: 100.0,
                qty: 1.0,
                side: TradeSide::Buy,
                trade_id: None,
                ts_ms: BUCKET_RETENTION_MS + 1_000_001,
            })
            .await;

        let guard = store.inner.read().await;
        assert!(
            guard
                .buckets
                .values()
                .all(|bucket| bucket.symbol != "OLDUSDT")
        );
        assert!(
            guard
                .cumulative_qty
                .keys()
                .all(|key| !key.contains("OLDUSDT"))
        );
        assert!(
            guard
                .cumulative_notional
                .keys()
                .all(|key| !key.contains("OLDUSDT"))
        );
    }

    #[tokio::test]
    async fn footprint_groups_trades_into_price_bins() {
        let store = OrderFlowStore::new(100_000.0);
        for (price, qty, side, ts_ms) in [
            (100.1, 2.0, TradeSide::Buy, 60_001),
            (100.2, 1.0, TradeSide::Sell, 60_002),
            (100.9, 3.0, TradeSide::Buy, 60_003),
        ] {
            store
                .update_trade(&TradeTick {
                    exchange: "binance",
                    market: MarketKind::Perp,
                    symbol: "BTCUSDT".into(),
                    price,
                    qty,
                    side,
                    trade_id: None,
                    ts_ms,
                })
                .await;
        }

        let rows = store
            .query_footprint(FootprintQuery {
                exchange: Some("binance".to_string()),
                market: Some("perp".to_string()),
                symbol: Some("BTCUSDT".to_string()),
                interval_ms: 60_000,
                scale: 1.0,
                start_ms: None,
                end_ms: None,
                imbalance_ratio: 3.0,
                imbalance_volume: 1.0,
                stacked_imbalance_range: 2,
                include_trades: true,
                limit: 10,
            })
            .await;

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bucket_start_ms, 60_000);
        assert_eq!(rows[0].total_trades, 3);
        assert_eq!(rows[0].trades.len(), 3);
        assert_eq!(rows[0].levels.len(), 2);
        assert_eq!(rows[0].delta_qty, 4.0);
    }

    #[tokio::test]
    async fn subscriber_loss_taints_order_flow_and_does_not_expire_with_new_trades() {
        let bus = crate::event_bus::EventBus::new_sharded(2, 10_000, 1);
        let store = OrderFlowStore::new(100_000.0);
        let stop = tokio_util::sync::CancellationToken::new();
        let task = super::spawn_order_flow_service(bus.clone(), store.clone(), stop.clone());
        let mut trade = TradeTick {
            exchange: "binance",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 1.0,
            side: TradeSide::Buy,
            trade_id: None,
            ts_ms: 60_000,
        };
        // The single-thread test runtime cannot poll the subscription forwarder
        // until this ring has lost exactly 14 entries.
        for ts in 0..16 {
            trade.ts_ms = 60_000 + ts;
            bus.publish_shared_event(std::sync::Arc::new(crate::types::DataEvent::Trade(
                trade.clone(),
            )));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !store.subscription_quality().await.has_known_gap {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(store.subscription_quality().await.skipped_events_total, 14);
        // Even after all original buckets are outside retention, CVD history
        // cannot be repaired by elapsed time or a later complete observation.
        trade.ts_ms += BUCKET_RETENTION_MS * 2;
        store.update_trade(&trade).await;
        let quality = store.subscription_quality().await;
        assert!(quality.has_known_gap);
        assert_eq!(quality.gap_epoch, 1);
        assert_eq!(
            serde_json::to_value(&quality).unwrap()["upstream_completeness"],
            "not_verified"
        );
        stop.cancel();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn global_memory_limits_cover_all_symbols_and_variable_trade_ids() {
        let limits = crate::config::runtime::MemoryLimits {
            order_flow_raw_records: 3,
            order_flow_raw_bytes: 4096,
            order_flow_keys: 2,
            order_flow_buckets: 9,
            ..Default::default()
        };
        let store = OrderFlowStore::with_limits(1000.0, limits.clone());
        for n in 0..20 {
            store
                .update_trade(&TradeTick {
                    exchange: "test",
                    market: MarketKind::Perp,
                    symbol: format!("S{n}").into(),
                    price: 10.0,
                    qty: 1.0,
                    side: TradeSide::Buy,
                    trade_id: Some(if n == 19 {
                        "x".repeat(8192).into()
                    } else {
                        "small".into()
                    }),
                    ts_ms: 1000 + n,
                })
                .await;
            let state = store.inner.read().await;
            assert!(state.raw_records <= limits.order_flow_raw_records);
            assert!(state.raw_bytes <= limits.order_flow_raw_bytes);
            assert!(state.raw_trades.len() <= limits.order_flow_keys);
            assert!(state.buckets.len() <= limits.order_flow_buckets);
            assert_eq!(
                state.raw_records,
                state
                    .raw_trades
                    .values()
                    .map(std::collections::VecDeque::len)
                    .sum::<usize>()
            );
            assert_eq!(
                state.raw_bytes,
                state
                    .raw_trades
                    .values()
                    .map(super::history_bytes)
                    .sum::<usize>()
            );
        }
        let state = store.inner.read().await;
        assert!(state.raw_retention_quality.has_capacity_loss);
        assert!(state.bucket_retention_quality.has_capacity_loss);
    }

    #[tokio::test]
    async fn global_retention_scan_is_amortized_and_accounting_survives_expiry() {
        let store = OrderFlowStore::new(1000.0);
        let mut trade = TradeTick {
            exchange: "test",
            market: MarketKind::Perp,
            symbol: "BTC".into(),
            price: 10.0,
            qty: 1.0,
            side: TradeSide::Buy,
            trade_id: None,
            ts_ms: 1000,
        };
        for ts in 1000..1100 {
            trade.ts_ms = ts;
            store.update_trade(&trade).await;
        }
        assert_eq!(store.inner.read().await.last_prune_ms, Some(1000));
        trade.ts_ms += RAW_TRADE_RETENTION_MS + 1000;
        store.update_trade(&trade).await;
        let state = store.inner.read().await;
        assert_eq!(state.raw_records, 1);
        assert_eq!(
            state.raw_bytes,
            state
                .raw_trades
                .values()
                .map(super::history_bytes)
                .sum::<usize>()
        );
        assert!(!state.raw_retention_quality.has_capacity_loss);
    }
}
