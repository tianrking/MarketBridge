use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::config::runtime::MemoryLimits;
use crate::data_quality::{RetentionQuality, SubscriptionQuality};
use crate::event_bus::EventBus;
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, LiquidationTick, MarketKind, MarketTick,
    OpenInterestTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const FLOW_WINDOW_MS: u64 = 60_000;
const LIQUIDATION_WINDOW_MS: u64 = 15 * 60_000;
const TRADE_FLOW_RETENTION_MS: u64 = LIQUIDATION_WINDOW_MS;
/// Longest rolling window exposed by the read-only squeeze research API.
/// This is deliberately bounded in memory; durable multi-day research belongs
/// in the configured data lake / replay workflow.
const ROLLING_HISTORY_MS: u64 = 24 * 60 * 60_000;
const NUMERIC_SAMPLE_INTERVAL_MS: u64 = 60_000;
const MAX_LIQUIDATION_EVENTS: usize = 10_000;
const MAX_ROLLING_SAMPLES: usize = 30_000;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolStateResponse {
    pub version: &'static str,
    pub domain: &'static str,
    pub symbol: String,
    pub exchange: Option<String>,
    pub generated_at_ms: u64,
    pub subscription_quality: SubscriptionQuality,
    pub retention_quality: RetentionQuality,
    pub source_quality: crate::ingestion::SourceIntegrity,
    pub states: Vec<StrategySymbolState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySymbolState {
    pub subscription_quality: SubscriptionQuality,
    pub retention_quality: RetentionQuality,
    pub source_quality: crate::ingestion::SourceIntegrity,
    pub exchange: String,
    pub symbol: String,
    pub generated_at_ms: u64,
    /// Most recent source observation consumed for this exchange/symbol pair.
    /// `generated_at_ms` is response construction time and must not be used as
    /// market-data freshness evidence.
    pub observed_at_ms: u64,
    pub metrics: StrategyMetrics,
    pub long_squeeze: StrategyLegState,
    pub short_exhaustion: StrategyLegState,
    pub risk_context: RiskContext,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StrategyMetrics {
    pub numeric_history_sampling_ms: u64,
    pub price_reference: &'static str,
    pub latest_price: Option<f64>,
    pub price_observed_at_ms: Option<u64>,
    pub funding_rate: Option<f64>,
    pub funding_observed_at_ms: Option<u64>,
    pub open_interest: Option<f64>,
    pub open_interest_value: Option<f64>,
    pub open_interest_observed_at_ms: Option<u64>,
    pub open_interest_change_pct: Option<f64>,
    /// Fixed-window changes include their actual sampled elapsed time. A null
    /// value means the service has not observed a suitably aligned baseline.
    pub open_interest_changes: Vec<RollingChange>,
    pub price_changes: Vec<RollingChange>,
    pub funding_observations_24h: usize,
    pub spot_cvd_notional_1m: Option<f64>,
    pub perp_cvd_notional_1m: Option<f64>,
    pub spot_volume_notional_15m: Option<f64>,
    pub perp_volume_notional_15m: Option<f64>,
    pub cvd_divergence: Option<String>,
    pub bid_depth_notional_10: Option<f64>,
    pub ask_depth_notional_10: Option<f64>,
    pub bid_ask_depth_ratio_10: Option<f64>,
    pub depth_pressure_10: Option<f64>,
    pub ofi_best_level_1m: Option<f64>,
    pub order_book_observed_at_ms: Option<u64>,
    pub buy_liquidation_notional_15m: Option<f64>,
    pub sell_liquidation_notional_15m: Option<f64>,
    pub liquidation_notional_15m: Option<f64>,
    pub liquidation_to_perp_volume_ratio_15m: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollingChange {
    pub window_ms: u64,
    pub actual_elapsed_ms: u64,
    pub change_pct: f64,
    pub baseline_ts_ms: u64,
    pub latest_ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyLegState {
    pub state: &'static str,
    pub score: i32,
    pub max_score: i32,
    pub progress: Vec<&'static str>,
    pub reasons: Vec<String>,
    pub invalidation: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RiskContext {
    pub suggested_hard_stop: Option<f64>,
    pub suggested_trailing_stop_bps: Option<f64>,
    pub execution_boundary: &'static str,
    pub notes: Vec<String>,
}

#[derive(Clone)]
pub struct StrategyStateStore {
    inner: Arc<RwLock<StrategyStateInner>>,
    limits: MemoryLimits,
    observability: Option<Arc<crate::metrics::AppMetrics>>,
}

#[derive(Default)]
struct StrategyStateInner {
    subscription_quality: SubscriptionQuality,
    retention_quality: RetentionQuality,
    accounted_bytes: usize,
    symbols: HashMap<String, SymbolRuntimeState>,
}

#[derive(Default)]
struct SymbolRuntimeState {
    accounted_bytes: usize,
    capacity_drops: usize,
    exchange: String,
    symbol: String,
    price_priority: u8,
    price_reference: &'static str,
    latest_price: Option<f64>,
    price_observed_at_ms: Option<u64>,
    funding_rate: Option<f64>,
    funding_observed_at_ms: Option<u64>,
    open_interest: Option<f64>,
    open_interest_value: Option<f64>,
    open_interest_observed_at_ms: Option<u64>,
    previous_open_interest: Option<f64>,
    open_interest_change_pct: Option<f64>,
    open_interest_history: VecDeque<NumericSample>,
    price_history: VecDeque<NumericSample>,
    funding_history: VecDeque<NumericSample>,
    spot_flow: VecDeque<FlowSample>,
    perp_flow: VecDeque<FlowSample>,
    liquidations: VecDeque<LiquidationSample>,
    latest_book: Option<BookSnapshot>,
    order_book_observed_at_ms: Option<u64>,
    previous_book_top: Option<BookTop>,
    ofi_samples: VecDeque<FlowSample>,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct FlowSample {
    ts_ms: u64,
    signed_notional: f64,
    notional: f64,
}

#[derive(Debug, Clone, Copy)]
struct LiquidationSample {
    ts_ms: u64,
    side: TradeSide,
    notional: f64,
}

#[derive(Debug, Clone, Copy)]
struct NumericSample {
    ts_ms: u64,
    value: f64,
}

#[derive(Debug, Clone)]
struct BookSnapshot {
    bid_depth_10: f64,
    ask_depth_10: f64,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct BookTop {
    bid_price: f64,
    bid_qty: f64,
    ask_price: f64,
    ask_qty: f64,
}

impl StrategyStateStore {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_limits(MemoryLimits::default())
    }

    pub fn with_limits(limits: MemoryLimits) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StrategyStateInner::default())),
            limits,
            observability: None,
        }
    }

    pub fn with_observability(mut self, metrics: Arc<crate::metrics::AppMetrics>) -> Self {
        self.observability = Some(metrics);
        self
    }
    pub fn source_quality(&self) -> crate::ingestion::SourceIntegrity {
        self.observability
            .as_ref()
            .map_or_else(Default::default, |m| m.ingestion.integrity())
    }

    pub async fn subscription_quality(&self) -> SubscriptionQuality {
        self.inner.read().await.subscription_quality.clone()
    }
    fn processed(&self, event: &crate::event_bus::SharedEvent) {
        if let (Some(metrics), Some(trace)) = (&self.observability, &event.trace) {
            metrics.ingestion.processed(trace, "strategy_state");
            metrics
                .pipeline_latency_seconds
                .with_label_values(&["strategy_state", crate::ingestion::domain(&event.event)])
                .observe(trace.received_at.elapsed().as_secs_f64());
        }
    }

    pub async fn retention_status(&self) -> serde_json::Value {
        let state = self.inner.read().await;
        serde_json::json!({"quality":state.retention_quality,"symbols":state.symbols.len(),
            "accounted_bytes":state.accounted_bytes,"limits":self.limits,
            "accounting":"owned symbol state and history capacities; not process RSS"})
    }

    pub async fn update_event(&self, event: &DataEvent) {
        match event {
            DataEvent::Tick(tick) => self.update_tick(tick).await,
            DataEvent::FundingRate(tick) => self.update_funding(tick).await,
            DataEvent::OpenInterest(tick) => self.update_open_interest(tick).await,
            DataEvent::Trade(tick) => self.update_trade(tick).await,
            DataEvent::Liquidation(tick) => self.update_liquidation(tick).await,
            DataEvent::OrderBook(tick) => self.update_order_book(tick).await,
            DataEvent::ExternalSignal(_) | DataEvent::Heartbeat { .. } => {}
        }
    }

    pub async fn query(&self, symbol: &str, exchange: Option<&str>) -> SymbolStateResponse {
        let now = now_ms();
        let symbol = symbol.to_ascii_uppercase();
        let exchange_filter = exchange.map(str::to_ascii_lowercase);
        let mut guard = self.inner.write().await;
        let source_quality = self.source_quality();
        let quality = guard.subscription_quality.clone();
        let retention_quality = guard.retention_quality.clone();
        let mut rows = guard
            .symbols
            .values_mut()
            .filter(|state| state.symbol.eq_ignore_ascii_case(&symbol))
            .filter(|state| {
                exchange_filter
                    .as_ref()
                    .is_none_or(|exchange| state.exchange.eq_ignore_ascii_case(exchange))
            })
            .map(|state| {
                state.prune(now);
                state.snapshot(now, &quality, &retention_quality, &source_quality)
            })
            .collect::<Vec<_>>();
        drop(guard);

        rows.sort_by(|a, b| a.exchange.cmp(&b.exchange).then(a.symbol.cmp(&b.symbol)));
        SymbolStateResponse {
            version: "v1",
            domain: "strategy_symbol_state",
            symbol,
            exchange: exchange_filter,
            generated_at_ms: now,
            subscription_quality: quality,
            retention_quality,
            source_quality,
            states: rows,
        }
    }

    /// Snapshot every observed symbol. This is intentionally a read-only
    /// in-memory view; absence from the response means no live data has been
    /// observed since this process started, not that a market does not exist.
    pub async fn query_all(&self) -> Vec<StrategySymbolState> {
        let now = now_ms();
        let mut guard = self.inner.write().await;
        let source_quality = self.source_quality();
        let quality = guard.subscription_quality.clone();
        let retention_quality = guard.retention_quality.clone();
        let mut rows = guard
            .symbols
            .values_mut()
            .map(|state| {
                state.prune(now);
                state.snapshot(now, &quality, &retention_quality, &source_quality)
            })
            .collect::<Vec<_>>();
        drop(guard);
        rows.sort_by(|a, b| a.exchange.cmp(&b.exchange).then(a.symbol.cmp(&b.symbol)));
        rows
    }

    async fn record_subscriber_gap(&self, skipped: u64) {
        self.inner
            .write()
            .await
            .subscription_quality
            .record_gap(skipped, now_ms());
    }

    async fn update_tick(&self, tick: &MarketTick) {
        if tick.market != MarketKind::Perp && tick.market != MarketKind::Spot {
            return;
        }
        let Some(mid) = mid(tick.bid, tick.ask) else {
            return;
        };
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        let (priority, reference, value) = if tick.market == MarketKind::Spot {
            (1, "spot_fallback", mid)
        } else if let Some(mark) = tick.mark {
            (3, "perpetual_mark", mark)
        } else {
            (4, "perpetual_midpoint", mid)
        };
        state.update_price(value, tick.ts_ms, priority, reference);
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }

    async fn update_funding(&self, tick: &FundingRateTick) {
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        state.funding_rate = Some(tick.funding_rate);
        state.funding_observed_at_ms = Some(tick.ts_ms);
        state.capacity_drops +=
            push_numeric(&mut state.funding_history, tick.ts_ms, tick.funding_rate);
        if let Some(mark) = tick.mark_price {
            state.update_price(mark, tick.ts_ms, 3, "perpetual_mark");
        }
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }

    async fn update_open_interest(&self, tick: &OpenInterestTick) {
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        if let Some(previous) = state.open_interest.filter(|value| *value > 0.0) {
            state.previous_open_interest = Some(previous);
            state.open_interest_change_pct =
                Some((tick.open_interest - previous) / previous * 100.0)
                    .filter(|value| value.is_finite());
        }
        state.open_interest = Some(tick.open_interest);
        state.open_interest_value = tick.open_interest_value;
        state.open_interest_observed_at_ms = Some(tick.ts_ms);
        state.capacity_drops += push_numeric(
            &mut state.open_interest_history,
            tick.ts_ms,
            tick.open_interest,
        );
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }

    async fn update_trade(&self, tick: &TradeTick) {
        let notional = tick.price * tick.qty;
        if !notional.is_finite() || notional <= 0.0 {
            return;
        }
        let signed = signed_notional(tick.side, tick.price, tick.qty);
        if !signed.is_finite() {
            return;
        }
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        let target = match tick.market {
            MarketKind::Spot => &mut state.spot_flow,
            MarketKind::Perp => &mut state.perp_flow,
        };
        target.push_back(FlowSample {
            ts_ms: tick.ts_ms,
            signed_notional: signed,
            notional,
        });
        let (priority, reference) = if tick.market == MarketKind::Perp {
            (2, "perpetual_trade")
        } else {
            (1, "spot_fallback")
        };
        state.update_price(tick.price, tick.ts_ms, priority, reference);
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        state.prune(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }

    async fn update_liquidation(&self, tick: &LiquidationTick) {
        let notional = tick.price * tick.qty;
        if !notional.is_finite() || notional <= 0.0 {
            return;
        }
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        state.liquidations.push_back(LiquidationSample {
            ts_ms: tick.ts_ms,
            side: tick.side,
            notional,
        });
        while state.liquidations.len() > MAX_LIQUIDATION_EVENTS {
            state.liquidations.pop_front();
            state.capacity_drops += 1;
        }
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        state.prune(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }

    async fn update_order_book(&self, tick: &OrderBookTick) {
        if tick.market != MarketKind::Perp {
            return;
        }
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        let current_top = book_top(tick);
        if let (Some(previous), Some(current)) = (state.previous_book_top, current_top) {
            let ofi = best_level_ofi(previous, current);
            if ofi.is_finite() {
                state.ofi_samples.push_back(FlowSample {
                    ts_ms: tick.ts_ms,
                    signed_notional: ofi,
                    notional: ofi.abs(),
                });
            }
        }
        state.previous_book_top = current_top;
        state.latest_book = Some(book_snapshot(tick));
        state.order_book_observed_at_ms = Some(tick.ts_ms);
        if let Some(price) = state
            .latest_book
            .as_ref()
            .and_then(|book| mid_opt(book.best_bid, book.best_ask))
        {
            state.update_price(price, tick.ts_ms, 4, "perpetual_midpoint");
        }
        state.updated_at_ms = state.updated_at_ms.max(tick.ts_ms);
        state.prune(tick.ts_ms);
        guard.finish_update(tick.exchange, &tick.symbol, &self.limits);
    }
}

impl StrategyStateInner {
    fn finish_update(&mut self, exchange: &str, symbol: &str, limits: &MemoryLimits) {
        let key = state_key(exchange, symbol);
        let state = self.symbols.get_mut(&key).unwrap();
        self.accounted_bytes -= state.accounted_bytes;
        state.accounted_bytes = state.owned_bytes() + key.capacity();
        self.accounted_bytes += state.accounted_bytes;
        self.retention_quality
            .record_eviction(std::mem::take(&mut state.capacity_drops));
        while self.symbols.len() > limits.strategy_symbols
            || self.accounted_bytes > limits.strategy_bytes
        {
            let key = self
                .symbols
                .iter()
                .min_by_key(|(key, state)| (state.updated_at_ms, *key))
                .map(|(key, _)| key.clone())
                .unwrap();
            let removed = self.symbols.remove(&key).unwrap();
            self.accounted_bytes -= removed.accounted_bytes;
            self.retention_quality
                .record_eviction(removed.sample_count().max(1));
        }
    }

    fn symbol_state(&mut self, exchange: &str, symbol: &str) -> &mut SymbolRuntimeState {
        let key = state_key(exchange, symbol);
        self.symbols
            .entry(key)
            .or_insert_with(|| SymbolRuntimeState {
                exchange: exchange.to_ascii_lowercase(),
                symbol: symbol.to_ascii_uppercase(),
                ..SymbolRuntimeState::default()
            })
    }
}

impl SymbolRuntimeState {
    fn update_price(&mut self, value: f64, ts: u64, priority: u8, reference: &'static str) {
        if priority < self.price_priority || !value.is_finite() || value <= 0.0 {
            return;
        }
        if priority > self.price_priority {
            self.price_history.clear();
            self.price_priority = priority;
            self.price_reference = reference;
        } else if self.price_observed_at_ms.is_some_and(|last| ts < last) {
            return;
        }
        self.latest_price = Some(value);
        self.price_observed_at_ms = Some(ts);
        self.capacity_drops += push_numeric(&mut self.price_history, ts, value);
    }

    fn sample_count(&self) -> usize {
        self.open_interest_history.len()
            + self.price_history.len()
            + self.funding_history.len()
            + self.spot_flow.len()
            + self.perp_flow.len()
            + self.ofi_samples.len()
            + self.liquidations.len()
    }

    fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.exchange.capacity()
            + self.symbol.capacity()
            + (self.open_interest_history.capacity()
                + self.price_history.capacity()
                + self.funding_history.capacity())
                * std::mem::size_of::<NumericSample>()
            + (self.spot_flow.capacity() + self.perp_flow.capacity() + self.ofi_samples.capacity())
                * std::mem::size_of::<FlowSample>()
            + self.liquidations.capacity() * std::mem::size_of::<LiquidationSample>()
    }

    fn prune(&mut self, now: u64) {
        prune_flow(&mut self.spot_flow, now, TRADE_FLOW_RETENTION_MS);
        prune_flow(&mut self.perp_flow, now, TRADE_FLOW_RETENTION_MS);
        prune_flow(&mut self.ofi_samples, now, FLOW_WINDOW_MS);
        while self
            .liquidations
            .front()
            .is_some_and(|sample| now.saturating_sub(sample.ts_ms) > LIQUIDATION_WINDOW_MS)
        {
            self.liquidations.pop_front();
        }
        prune_numeric(&mut self.open_interest_history, now);
        prune_numeric(&mut self.price_history, now);
        prune_numeric(&mut self.funding_history, now);
    }

    fn snapshot(
        &self,
        now: u64,
        quality: &SubscriptionQuality,
        retention: &RetentionQuality,
        source: &crate::ingestion::SourceIntegrity,
    ) -> StrategySymbolState {
        let metrics = self.metrics(now);
        let mut long_squeeze = long_squeeze_state(&metrics);
        let mut short_exhaustion = short_exhaustion_state(&metrics);
        if quality.has_known_gap || retention.has_capacity_loss || source.has_known_loss {
            for leg in [&mut long_squeeze, &mut short_exhaustion] {
                leg.state = "incomplete_data";
                leg.score = 0;
                leg.progress.clear();
                leg.invalidation
                    .push("process-local subscription or retention lost observations; state rebuild required".into());
            }
        }
        let risk_context = risk_context(self.latest_price, &long_squeeze, &short_exhaustion);
        StrategySymbolState {
            subscription_quality: quality.clone(),
            retention_quality: retention.clone(),
            source_quality: source.clone(),
            exchange: self.exchange.clone(),
            symbol: self.symbol.clone(),
            generated_at_ms: now,
            observed_at_ms: self.updated_at_ms,
            metrics,
            long_squeeze,
            short_exhaustion,
            risk_context,
        }
    }

    fn metrics(&self, now: u64) -> StrategyMetrics {
        let spot_cvd = sum_flow(&self.spot_flow, now, FLOW_WINDOW_MS);
        let perp_cvd = sum_flow(&self.perp_flow, now, FLOW_WINDOW_MS);
        let spot_volume = sum_volume(&self.spot_flow, now, LIQUIDATION_WINDOW_MS);
        let perp_volume = sum_volume(&self.perp_flow, now, LIQUIDATION_WINDOW_MS);
        let (buy_liq, sell_liq, total_liq) = liquidation_totals(&self.liquidations);
        let liquidation_to_perp_volume_ratio = total_liq
            .zip(perp_volume)
            .filter(|(_, volume)| *volume > 0.0)
            .map(|(liquidation, volume)| liquidation / volume)
            .filter(|ratio| ratio.is_finite());
        let (bid_depth, ask_depth, ratio, pressure) = self
            .latest_book
            .as_ref()
            .map(|book| {
                let ratio =
                    (book.ask_depth_10 > 0.0).then_some(book.bid_depth_10 / book.ask_depth_10);
                let total = book.bid_depth_10 + book.ask_depth_10;
                let pressure =
                    (total > 0.0).then_some((book.bid_depth_10 - book.ask_depth_10) / total);
                (
                    Some(book.bid_depth_10),
                    Some(book.ask_depth_10),
                    ratio,
                    pressure,
                )
            })
            .unwrap_or((None, None, None, None));
        let cvd_divergence = match (spot_cvd, perp_cvd) {
            (Some(spot), Some(perp)) if spot > 0.0 && perp < 0.0 => {
                Some("spot_up_perp_down".to_string())
            }
            (Some(spot), Some(perp)) if spot < 0.0 && perp > 0.0 => {
                Some("spot_down_perp_up".to_string())
            }
            (Some(spot), Some(perp)) if spot > 0.0 && perp > 0.0 => Some("both_up".to_string()),
            (Some(spot), Some(perp)) if spot < 0.0 && perp < 0.0 => Some("both_down".to_string()),
            (Some(_), Some(_)) => Some("flat_or_mixed".to_string()),
            _ => None,
        };
        StrategyMetrics {
            numeric_history_sampling_ms: NUMERIC_SAMPLE_INTERVAL_MS,
            price_reference: self.price_reference,
            latest_price: self.latest_price,
            price_observed_at_ms: self.price_observed_at_ms,
            funding_rate: self.funding_rate,
            funding_observed_at_ms: self.funding_observed_at_ms,
            open_interest: self.open_interest,
            open_interest_value: self.open_interest_value,
            open_interest_observed_at_ms: self.open_interest_observed_at_ms,
            open_interest_change_pct: self.open_interest_change_pct,
            open_interest_changes: rolling_changes(&self.open_interest_history),
            price_changes: rolling_changes(&self.price_history),
            funding_observations_24h: self
                .funding_history
                .iter()
                .filter(|s| now.saturating_sub(s.ts_ms) <= ROLLING_HISTORY_MS)
                .count(),
            spot_cvd_notional_1m: spot_cvd,
            perp_cvd_notional_1m: perp_cvd,
            spot_volume_notional_15m: spot_volume,
            perp_volume_notional_15m: perp_volume,
            cvd_divergence,
            bid_depth_notional_10: bid_depth,
            ask_depth_notional_10: ask_depth,
            bid_ask_depth_ratio_10: ratio,
            depth_pressure_10: pressure,
            ofi_best_level_1m: sum_flow(&self.ofi_samples, now, FLOW_WINDOW_MS),
            order_book_observed_at_ms: self.order_book_observed_at_ms,
            buy_liquidation_notional_15m: buy_liq,
            sell_liquidation_notional_15m: sell_liq,
            liquidation_notional_15m: total_liq,
            liquidation_to_perp_volume_ratio_15m: liquidation_to_perp_volume_ratio,
        }
    }
}

const ROLLING_WINDOWS_MS: [u64; 5] = [
    5 * 60_000,
    15 * 60_000,
    60 * 60_000,
    4 * 60 * 60_000,
    24 * 60 * 60_000,
];

fn push_numeric(samples: &mut VecDeque<NumericSample>, ts_ms: u64, value: f64) -> usize {
    if ts_ms == 0 || !value.is_finite() {
        return 0;
    }
    if samples
        .back()
        .is_some_and(|previous| ts_ms < previous.ts_ms)
    {
        // Out-of-order samples are not used for historical window baselines;
        // accepting them would create look-ahead errors in a replay.
        return 0;
    }
    if samples.back().is_some_and(|previous| {
        previous.ts_ms / NUMERIC_SAMPLE_INTERVAL_MS == ts_ms / NUMERIC_SAMPLE_INTERVAL_MS
    }) {
        samples.pop_back();
    }
    samples.push_back(NumericSample { ts_ms, value });
    let mut removed = 0;
    while samples.len() > MAX_ROLLING_SAMPLES {
        samples.pop_front();
        removed += 1;
    }
    removed
}

fn prune_numeric(samples: &mut VecDeque<NumericSample>, now: u64) {
    while samples.front().is_some_and(|sample| {
        now.saturating_sub(sample.ts_ms) > ROLLING_HISTORY_MS + 2 * NUMERIC_SAMPLE_INTERVAL_MS
    }) {
        samples.pop_front();
    }
}

fn rolling_changes(samples: &VecDeque<NumericSample>) -> Vec<RollingChange> {
    let Some(latest) = samples.back().copied() else {
        return Vec::new();
    };
    ROLLING_WINDOWS_MS
        .into_iter()
        .filter_map(|window_ms| {
            if samples
                .front()
                .is_none_or(|first| latest.ts_ms.saturating_sub(first.ts_ms) < window_ms)
            {
                return None;
            }
            let target = latest.ts_ms.saturating_sub(window_ms);
            // Require the full warmup, then allow only minute-grid jitter
            // or 1% on longer windows. Actual elapsed time remains explicit.
            let tolerance = (window_ms / 100).max(NUMERIC_SAMPLE_INTERVAL_MS);
            let baseline = samples
                .iter()
                .filter(|sample| sample.ts_ms <= latest.ts_ms)
                .min_by_key(|sample| sample.ts_ms.abs_diff(target))?;
            let elapsed = latest.ts_ms.saturating_sub(baseline.ts_ms);
            if baseline.value <= 0.0
                || elapsed < window_ms.saturating_sub(tolerance)
                || elapsed > window_ms.saturating_add(tolerance)
            {
                return None;
            }
            let change_pct = (latest.value - baseline.value) / baseline.value * 100.0;
            change_pct.is_finite().then_some(RollingChange {
                window_ms,
                actual_elapsed_ms: elapsed,
                change_pct,
                baseline_ts_ms: baseline.ts_ms,
                latest_ts_ms: latest.ts_ms,
            })
        })
        .collect()
}

pub fn spawn_strategy_state_service(
    bus: EventBus,
    store: StrategyStateStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    let mut rx = bus.subscribe_events();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                result = rx.recv() => match result {
                    Ok(shared) => {store.update_event(shared.event.as_ref()).await;store.processed(&shared);},
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        store.record_subscriber_gap(skipped).await;
                        warn!(skipped, "strategy state subscriber lagged; research triggers disabled");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    })
}

fn long_squeeze_state(metrics: &StrategyMetrics) -> StrategyLegState {
    let mut score = 0;
    let mut progress = Vec::new();
    let mut reasons = Vec::new();
    let mut invalidation = Vec::new();

    if metrics.funding_rate.is_some_and(|rate| rate <= -0.0005) {
        score += 2;
        progress.push("crowded_shorts");
        reasons.push(format!(
            "funding deeply negative: {:.4}%",
            metrics.funding_rate.unwrap_or_default() * 100.0
        ));
    } else if metrics.funding_rate.is_some_and(|rate| rate < 0.0) {
        score += 1;
        progress.push("short_bias");
    }

    if metrics
        .open_interest_change_pct
        .is_some_and(|change| change >= 15.0)
    {
        score += 3;
        progress.push("oi_stack");
        reasons.push(format!(
            "OI expansion is extreme: +{:.2}%",
            metrics.open_interest_change_pct.unwrap_or_default()
        ));
    } else if metrics
        .open_interest_change_pct
        .is_some_and(|change| change >= 3.0)
    {
        score += 1;
        progress.push("oi_rising");
    }

    if metrics.cvd_divergence.as_deref() == Some("spot_up_perp_down") {
        score += 3;
        progress.push("spot_absorption");
        reasons.push("spot CVD up while perp CVD down".to_string());
    }

    if metrics.ofi_best_level_1m.is_some_and(|ofi| ofi > 0.0) {
        score += 1;
        progress.push("book_confirming");
    }

    if metrics
        .buy_liquidation_notional_15m
        .is_some_and(|value| value > 0.0)
    {
        score += 1;
        progress.push("liquidation_fuel");
    }

    if metrics
        .depth_pressure_10
        .is_some_and(|pressure| pressure < -0.4)
    {
        invalidation.push(
            "top-10 book is ask-heavy; squeeze needs aggressive buy confirmation".to_string(),
        );
    }

    let state = if score >= 8 {
        "triggered_long_squeeze"
    } else if progress.contains(&"spot_absorption") {
        "spot_absorption"
    } else if progress.contains(&"oi_stack") || progress.contains(&"oi_rising") {
        "chip_accumulation"
    } else if progress.contains(&"crowded_shorts") || progress.contains(&"short_bias") {
        "short_crowding"
    } else {
        "neutral"
    };

    StrategyLegState {
        state,
        score,
        max_score: 10,
        progress,
        reasons,
        invalidation,
    }
}

fn short_exhaustion_state(metrics: &StrategyMetrics) -> StrategyLegState {
    let mut score = 0;
    let mut progress = Vec::new();
    let mut reasons = Vec::new();
    let mut invalidation = Vec::new();

    if metrics.funding_rate.is_some_and(|rate| rate >= 0.0005) {
        score += 2;
        progress.push("crowded_longs");
        reasons.push(format!(
            "funding extremely positive: {:.4}%",
            metrics.funding_rate.unwrap_or_default() * 100.0
        ));
    } else if metrics.funding_rate.is_some_and(|rate| rate > 0.0) {
        score += 1;
        progress.push("long_bias");
    }

    if metrics
        .open_interest_change_pct
        .is_some_and(|change| change <= -10.0)
    {
        score += 3;
        progress.push("fuel_exhausted");
        reasons.push(format!(
            "OI dropped sharply: {:.2}%",
            metrics.open_interest_change_pct.unwrap_or_default()
        ));
    } else if metrics
        .open_interest_change_pct
        .is_some_and(|change| change < 0.0)
    {
        score += 1;
        progress.push("oi_slipping");
    }

    if metrics.perp_cvd_notional_1m.is_some_and(|cvd| cvd < 0.0) {
        score += 1;
        progress.push("sell_flow_confirming");
    }

    if metrics
        .bid_ask_depth_ratio_10
        .is_some_and(|ratio| ratio < 0.65)
    {
        score += 2;
        progress.push("book_vacuum");
        reasons.push(format!(
            "bid depth is thin versus asks: {:.2}",
            metrics.bid_ask_depth_ratio_10.unwrap_or_default()
        ));
    } else if metrics
        .bid_ask_depth_ratio_10
        .is_some_and(|ratio| ratio < 1.0)
    {
        score += 1;
        progress.push("bids_weaker_than_asks");
    }

    if metrics.ofi_best_level_1m.is_some_and(|ofi| ofi < 0.0) {
        score += 1;
        progress.push("ofi_negative");
    }

    if metrics
        .buy_liquidation_notional_15m
        .is_some_and(|value| value > 0.0)
    {
        invalidation.push("recent short liquidation fuel still exists; avoid early short without failed breakout confirmation".to_string());
    }

    let state = if score >= 8 {
        "triggered_short_exhaustion"
    } else if progress.contains(&"book_vacuum") {
        "book_vacuum"
    } else if progress.contains(&"fuel_exhausted") || progress.contains(&"oi_slipping") {
        "fuel_exhaustion"
    } else if progress.contains(&"crowded_longs") || progress.contains(&"long_bias") {
        "long_crowding"
    } else {
        "neutral"
    };

    StrategyLegState {
        state,
        score,
        max_score: 10,
        progress,
        reasons,
        invalidation,
    }
}

fn risk_context(
    price: Option<f64>,
    long_squeeze: &StrategyLegState,
    short_exhaustion: &StrategyLegState,
) -> RiskContext {
    let active = long_squeeze.state == "triggered_long_squeeze"
        || short_exhaustion.state == "triggered_short_exhaustion";
    let suggested_hard_stop = if active {
        price.map(|price| {
            if long_squeeze.state == "triggered_long_squeeze" {
                price * 0.985
            } else {
                price * 1.015
            }
        })
    } else {
        None
    };
    RiskContext {
        suggested_hard_stop,
        suggested_trailing_stop_bps: active.then_some(150.0),
        execution_boundary: "read_only_signal_context_no_order_execution",
        notes: vec![
            "Hard stop here is a live-context fallback; production execution should anchor stops to the impulse candle or failed-breakout candle.".to_string(),
            "MarketBridge does not place orders. Wire this output into a separate execution/risk service for live trading.".to_string(),
        ],
    }
}

fn prune_flow(samples: &mut VecDeque<FlowSample>, now: u64, window_ms: u64) {
    while samples
        .front()
        .is_some_and(|sample| now.saturating_sub(sample.ts_ms) > window_ms)
    {
        samples.pop_front();
    }
}

fn sum_flow(samples: &VecDeque<FlowSample>, now: u64, window_ms: u64) -> Option<f64> {
    let total: f64 = samples
        .iter()
        .filter(|sample| now.saturating_sub(sample.ts_ms) <= window_ms)
        .map(|sample| sample.signed_notional)
        .sum();
    samples
        .iter()
        .any(|sample| now.saturating_sub(sample.ts_ms) <= window_ms)
        .then_some(total)
}

fn sum_volume(samples: &VecDeque<FlowSample>, now: u64, window_ms: u64) -> Option<f64> {
    let total: f64 = samples
        .iter()
        .filter(|sample| now.saturating_sub(sample.ts_ms) <= window_ms)
        .map(|sample| sample.notional)
        .sum();
    samples
        .iter()
        .any(|sample| now.saturating_sub(sample.ts_ms) <= window_ms)
        .then_some(total)
}

fn liquidation_totals(
    samples: &VecDeque<LiquidationSample>,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    if samples.is_empty() {
        return (None, None, None);
    }
    let mut buy = 0.0;
    let mut sell = 0.0;
    for sample in samples {
        match sample.side {
            TradeSide::Buy => buy += sample.notional,
            TradeSide::Sell => sell += sample.notional,
            TradeSide::Unknown => {}
        }
    }
    (
        (buy > 0.0).then_some(buy),
        (sell > 0.0).then_some(sell),
        (buy + sell > 0.0).then_some(buy + sell),
    )
}

fn signed_notional(side: TradeSide, price: f64, qty: f64) -> f64 {
    let notional = price * qty;
    match side {
        TradeSide::Buy => notional,
        TradeSide::Sell => -notional,
        TradeSide::Unknown => 0.0,
    }
}

fn book_snapshot(book: &OrderBookTick) -> BookSnapshot {
    let bid_depth_10 = depth_notional(&book.bids, 10);
    let ask_depth_10 = depth_notional(&book.asks, 10);
    BookSnapshot {
        bid_depth_10,
        ask_depth_10,
        best_bid: book.bids.first().map(|level| level.price),
        best_ask: book.asks.first().map(|level| level.price),
    }
}

fn book_top(book: &OrderBookTick) -> Option<BookTop> {
    let bid = book.bids.first()?;
    let ask = book.asks.first()?;
    Some(BookTop {
        bid_price: bid.price,
        bid_qty: bid.qty,
        ask_price: ask.price,
        ask_qty: ask.qty,
    })
}

fn best_level_ofi(previous: BookTop, current: BookTop) -> f64 {
    let bid_component = if current.bid_price > previous.bid_price {
        current.bid_qty
    } else if current.bid_price < previous.bid_price {
        -previous.bid_qty
    } else {
        current.bid_qty - previous.bid_qty
    };
    let ask_component = if current.ask_price < previous.ask_price {
        current.ask_qty
    } else if current.ask_price > previous.ask_price {
        -previous.ask_qty
    } else {
        current.ask_qty - previous.ask_qty
    };
    let mid = mid(current.bid_price, current.ask_price).unwrap_or(1.0);
    (bid_component - ask_component) * mid
}

fn depth_notional(levels: &[BookLevel], count: usize) -> f64 {
    levels
        .iter()
        .take(count)
        .filter(|level| level.price > 0.0 && level.qty > 0.0)
        .map(|level| level.price * level.qty)
        .filter(|value| value.is_finite())
        .sum()
}

fn mid(bid: f64, ask: f64) -> Option<f64> {
    (bid > 0.0 && ask >= bid)
        .then_some((bid + ask) / 2.0)
        .filter(|value| value.is_finite())
}

fn mid_opt(bid: Option<f64>, ask: Option<f64>) -> Option<f64> {
    mid(bid?, ask?)
}

fn state_key(exchange: &str, symbol: &str) -> String {
    format!(
        "{}:{}",
        exchange.to_ascii_lowercase(),
        symbol.to_ascii_uppercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squeeze_state_advances_on_core_confluence() {
        let metrics = StrategyMetrics {
            funding_rate: Some(-0.0006),
            open_interest_change_pct: Some(16.0),
            cvd_divergence: Some("spot_up_perp_down".to_string()),
            ofi_best_level_1m: Some(1.0),
            buy_liquidation_notional_15m: Some(1000.0),
            ..StrategyMetrics::default()
        };

        let state = long_squeeze_state(&metrics);

        assert_eq!(state.state, "triggered_long_squeeze");
        assert!(state.progress.contains(&"spot_absorption"));
    }

    #[test]
    fn short_state_advances_on_crowding_and_book_vacuum() {
        let metrics = StrategyMetrics {
            funding_rate: Some(0.0006),
            open_interest_change_pct: Some(-12.0),
            perp_cvd_notional_1m: Some(-1000.0),
            bid_ask_depth_ratio_10: Some(0.5),
            ofi_best_level_1m: Some(-100.0),
            ..StrategyMetrics::default()
        };

        let state = short_exhaustion_state(&metrics);

        assert_eq!(state.state, "triggered_short_exhaustion");
        assert!(state.progress.contains(&"book_vacuum"));
    }

    #[test]
    fn rolling_changes_require_an_aligned_baseline_and_report_actual_elapsed_time() {
        let mut samples = VecDeque::new();
        push_numeric(&mut samples, 1_000, 100.0);
        push_numeric(&mut samples, 3_601_000, 120.0);
        let changes = rolling_changes(&samples);
        let one_hour = changes
            .iter()
            .find(|change| change.window_ms == 60 * 60_000)
            .unwrap();
        assert_eq!(one_hour.actual_elapsed_ms, 60 * 60_000);
        assert!((one_hour.change_pct - 20.0).abs() < f64::EPSILON);

        let mut sparse = VecDeque::new();
        push_numeric(&mut sparse, 1_000, 100.0);
        push_numeric(&mut sparse, 8 * 60 * 60_000 + 1_000, 120.0);
        assert!(
            rolling_changes(&sparse)
                .iter()
                .all(|change| change.window_ms != 60 * 60_000)
        );
    }

    #[test]
    fn liquidation_ratio_uses_same_window_perp_volume() {
        let now = 900_000;
        let mut state = SymbolRuntimeState::default();
        state.perp_flow.push_back(FlowSample {
            ts_ms: now - 1_000,
            signed_notional: 8_000.0,
            notional: 8_000.0,
        });
        state.perp_flow.push_back(FlowSample {
            ts_ms: now - 500,
            signed_notional: 0.0,
            notional: 2_000.0,
        });
        state.liquidations.push_back(LiquidationSample {
            ts_ms: now - 2_000,
            side: TradeSide::Buy,
            notional: 2_000.0,
        });
        let metrics = state.metrics(now);
        assert_eq!(metrics.perp_volume_notional_15m, Some(10_000.0));
        assert_eq!(metrics.liquidation_notional_15m, Some(2_000.0));
        assert_eq!(metrics.liquidation_to_perp_volume_ratio_15m, Some(0.2));
    }

    #[tokio::test]
    async fn subscriber_loss_is_serialized_for_all_symbols_and_blocks_legacy_triggers() {
        let bus = EventBus::new_sharded(2, 10_000, 1);
        let store = StrategyStateStore::new();
        let stop = CancellationToken::new();
        let task = spawn_strategy_state_service(bus.clone(), store.clone(), stop.clone());
        // Heartbeat loss has no symbol attribution in this subscription, so
        // even a symbol first observed later must carry the global gap.
        for ts in 0..16 {
            bus.publish_shared_event(Arc::new(DataEvent::Heartbeat {
                exchange: "test",
                ts_ms: ts,
            }));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !store
                .query("BTCUSDT", None)
                .await
                .subscription_quality
                .has_known_gap
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for symbol in ["BTCUSDT", "ETHUSDT"] {
            store
                .update_event(&DataEvent::Trade(TradeTick {
                    exchange: "binance",
                    market: MarketKind::Perp,
                    symbol: symbol.into(),
                    price: 100.0,
                    qty: 1.0,
                    side: TradeSide::Buy,
                    trade_id: None,
                    ts_ms: now_ms(),
                }))
                .await;
        }
        let rows = store.query_all().await;
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(row.subscription_quality.has_known_gap);
            assert_eq!(row.subscription_quality.skipped_events_total, 14);
            assert_eq!(row.long_squeeze.state, "incomplete_data");
            assert_eq!(row.short_exhaustion.state, "incomplete_data");
            assert_eq!(row.long_squeeze.score, 0);
        }
        let response = serde_json::to_value(store.query("BTCUSDT", None).await).unwrap();
        assert_eq!(response["subscription_quality"]["gap_epoch"], 1);
        assert_eq!(
            response["states"][0]["subscription_quality"]["has_known_gap"],
            true
        );
        stop.cancel();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn closed_subscription_exits_instead_of_spinning() {
        let bus = EventBus::new_sharded(2, 10_000, 1);
        let task = spawn_strategy_state_service(
            bus.clone(),
            StrategyStateStore::new(),
            CancellationToken::new(),
        );
        drop(bus);
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn global_symbol_and_allocated_history_budgets_mark_loss_and_disable_legs() {
        let limits = MemoryLimits {
            strategy_symbols: 1,
            strategy_bytes: 4096,
            ..Default::default()
        };
        let store = StrategyStateStore::with_limits(limits);
        for symbol in ["BTC", "ETH", "SOL"] {
            store
                .update_event(&DataEvent::Trade(TradeTick {
                    exchange: "test",
                    market: MarketKind::Perp,
                    symbol: symbol.into(),
                    price: 10.0,
                    qty: 1.0,
                    side: TradeSide::Buy,
                    trade_id: None,
                    ts_ms: now_ms(),
                }))
                .await;
        }
        let state = store.inner.read().await;
        assert!(state.symbols.len() <= 1);
        assert!(state.accounted_bytes <= 4096);
        assert_eq!(
            state.accounted_bytes,
            state
                .symbols
                .values()
                .map(|s| s.accounted_bytes)
                .sum::<usize>()
        );
        assert!(state.retention_quality.has_capacity_loss);
        drop(state);
        for row in store.query_all().await {
            assert!(row.retention_quality.has_capacity_loss);
            assert_eq!(row.long_squeeze.state, "incomplete_data");
        }
        let tiny = StrategyStateStore::with_limits(MemoryLimits {
            strategy_bytes: 1,
            ..Default::default()
        });
        tiny.update_event(&DataEvent::FundingRate(FundingRateTick {
            exchange: "test",
            symbol: "BTC".into(),
            funding_rate: 0.01,
            next_funding_time_ms: None,
            mark_price: None,
            index_price: None,
            ts_ms: now_ms(),
        }))
        .await;
        assert!(tiny.query_all().await.is_empty());
        assert_eq!(tiny.inner.read().await.accounted_bytes, 0);
        assert!(
            tiny.query("BTC", None)
                .await
                .retention_quality
                .has_capacity_loss
        );
    }

    #[tokio::test]
    async fn perpetual_price_history_is_not_overwritten_by_spot_or_liquidation_prints() {
        let store = StrategyStateStore::new();
        let ts = now_ms();
        for (market, price, time) in [
            (MarketKind::Spot, 100.0, ts),
            (MarketKind::Perp, 200.0, ts + 1),
            (MarketKind::Spot, 1000.0, ts + 2),
        ] {
            store
                .update_event(&DataEvent::Tick(MarketTick {
                    exchange: "test",
                    market,
                    symbol: "BTC".into(),
                    bid: price - 1.0,
                    ask: price + 1.0,
                    mark: None,
                    funding_rate: None,
                    ts_ms: time,
                }))
                .await;
        }
        store
            .update_event(&DataEvent::Liquidation(LiquidationTick {
                exchange: "test",
                symbol: "BTC".into(),
                price: 1.0,
                qty: 1.0,
                side: TradeSide::Buy,
                ts_ms: ts + 3,
            }))
            .await;
        let row = store.query("BTC", None).await.states.remove(0);
        assert_eq!(row.metrics.latest_price, Some(200.0));
        assert_eq!(row.metrics.price_reference, "perpetual_midpoint");
        assert_eq!(row.metrics.price_observed_at_ms, Some(ts + 1));
    }
    #[test]
    fn minute_history_sampling_preserves_full_warmup_and_declared_resolution() {
        let mut state = SymbolRuntimeState::default();
        for ts in 1000..31_000 {
            state.update_price(100.0, ts, 4, "perpetual_midpoint");
        }
        assert_eq!(state.price_history.len(), 1);
        assert_eq!(state.capacity_drops, 0);
        for minute in 1..=44 {
            state.update_price(110.0, minute * 60_000, 4, "perpetual_midpoint");
        }
        state.update_price(110.0, 45 * 60_000, 4, "perpetual_midpoint");
        assert!(
            rolling_changes(&state.price_history)
                .iter()
                .all(|c| c.window_ms != 60 * 60_000),
            "45m must not masquerade as 1h"
        );
        for minute in 46..=62 {
            state.update_price(110.0, minute * 60_000, 4, "perpetual_midpoint");
        }
        assert!(
            rolling_changes(&state.price_history)
                .iter()
                .any(|c| c.window_ms == 60 * 60_000)
        );
        assert_eq!(
            state.metrics(62 * 60_000).numeric_history_sampling_ms,
            60_000
        );
    }
}
