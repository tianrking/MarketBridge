use std::collections::HashMap;
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
    pub large_trade_count: u64,
    pub delta_qty: f64,
    pub delta_notional: f64,
    pub cumulative_delta_qty: f64,
    pub cumulative_delta_notional: f64,
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
                large_trade_count: 0,
                delta_qty: 0.0,
                delta_notional: 0.0,
                cumulative_delta_qty,
                cumulative_delta_notional,
                updated_at_ms: crate::types::now_ms(),
            });
            match trade.side {
                TradeSide::Buy => {
                    bucket.buy_qty += trade.qty;
                    bucket.buy_notional += notional;
                }
                TradeSide::Sell => {
                    bucket.sell_qty += trade.qty;
                    bucket.sell_notional += notional;
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

#[cfg(test)]
mod tests {
    use super::{MAX_BUCKETS_PER_KEY, OrderFlowQuery, OrderFlowStore};
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
}
