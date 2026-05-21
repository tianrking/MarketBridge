use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::event_bus::{EventBus, EventDomain};
use crate::types::{DataEvent, TradeSide, TradeTick};

const DEFAULT_WINDOWS_MS: &[u64] = &[60_000, 300_000, 900_000];
const MAX_BUCKETS_PER_KEY: usize = 500;
const MAX_RAW_TRADES_PER_KEY: usize = 20_000;

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
}

#[derive(Default)]
struct OrderFlowState {
    buckets: HashMap<String, OrderFlowBucket>,
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
    pub fn new(large_trade_notional_usdt: f64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(OrderFlowState::default())),
            large_trade_notional_usdt,
        }
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
        let raw = guard.raw_trades.entry(raw_key).or_default();
        raw.push_back(StoredTrade {
            exchange: trade.exchange.to_string(),
            market: market.clone(),
            symbol: symbol.clone(),
            ts_ms: trade.ts_ms,
            price: trade.price,
            qty: trade.qty,
            side: trade.side,
            trade_id: trade.trade_id.as_ref().map(|id| id.to_string()),
            notional,
        });
        while raw.len() > MAX_RAW_TRADES_PER_KEY {
            raw.pop_front();
        }

        for window_ms in DEFAULT_WINDOWS_MS {
            let bucket_start_ms = trade.ts_ms / window_ms * window_ms;
            let bucket_end_ms = bucket_start_ms + window_ms - 1;
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
            let cumulative_notional = guard.cumulative_notional.entry(cvd_key).or_default();
            *cumulative_notional += delta_sign * notional;
            let cumulative_delta_notional = *cumulative_notional;

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
        }
        prune_buckets(&mut guard.buckets);
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
    tokio::spawn(async move {
        let mut rx = bus.subscribe_domain(EventDomain::Trade);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                received = rx.recv() => {
                    match received {
                        Ok(event) => {
                            if let DataEvent::Trade(trade) = event.as_ref() {
                                store.update_trade(trade).await;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
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
    use super::{FootprintQuery, MAX_BUCKETS_PER_KEY, OrderFlowQuery, OrderFlowStore};
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
}
