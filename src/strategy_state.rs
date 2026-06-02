use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::event_bus::EventBus;
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, LiquidationTick, MarketKind, MarketTick,
    OpenInterestTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const FLOW_WINDOW_MS: u64 = 60_000;
const LIQUIDATION_WINDOW_MS: u64 = 15 * 60_000;
const MAX_FLOW_EVENTS_PER_SIDE: usize = 20_000;
const MAX_LIQUIDATION_EVENTS: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolStateResponse {
    pub version: &'static str,
    pub domain: &'static str,
    pub symbol: String,
    pub exchange: Option<String>,
    pub generated_at_ms: u64,
    pub states: Vec<StrategySymbolState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategySymbolState {
    pub exchange: String,
    pub symbol: String,
    pub generated_at_ms: u64,
    pub metrics: StrategyMetrics,
    pub long_squeeze: StrategyLegState,
    pub short_exhaustion: StrategyLegState,
    pub risk_context: RiskContext,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StrategyMetrics {
    pub latest_price: Option<f64>,
    pub funding_rate: Option<f64>,
    pub open_interest: Option<f64>,
    pub open_interest_value: Option<f64>,
    pub open_interest_change_pct: Option<f64>,
    pub spot_cvd_notional_1m: Option<f64>,
    pub perp_cvd_notional_1m: Option<f64>,
    pub cvd_divergence: Option<String>,
    pub bid_depth_notional_10: Option<f64>,
    pub ask_depth_notional_10: Option<f64>,
    pub bid_ask_depth_ratio_10: Option<f64>,
    pub depth_pressure_10: Option<f64>,
    pub ofi_best_level_1m: Option<f64>,
    pub buy_liquidation_notional_15m: Option<f64>,
    pub sell_liquidation_notional_15m: Option<f64>,
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
}

#[derive(Default)]
struct StrategyStateInner {
    symbols: HashMap<String, SymbolRuntimeState>,
}

#[derive(Default)]
struct SymbolRuntimeState {
    exchange: String,
    symbol: String,
    latest_price: Option<f64>,
    funding_rate: Option<f64>,
    open_interest: Option<f64>,
    open_interest_value: Option<f64>,
    previous_open_interest: Option<f64>,
    open_interest_change_pct: Option<f64>,
    spot_flow: VecDeque<FlowSample>,
    perp_flow: VecDeque<FlowSample>,
    liquidations: VecDeque<LiquidationSample>,
    latest_book: Option<BookSnapshot>,
    previous_book_top: Option<BookTop>,
    ofi_samples: VecDeque<FlowSample>,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct FlowSample {
    ts_ms: u64,
    signed_notional: f64,
}

#[derive(Debug, Clone, Copy)]
struct LiquidationSample {
    ts_ms: u64,
    side: TradeSide,
    notional: f64,
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
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StrategyStateInner::default())),
        }
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
        let mut rows = self
            .inner
            .write()
            .await
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
                state.snapshot(now)
            })
            .collect::<Vec<_>>();

        rows.sort_by(|a, b| a.exchange.cmp(&b.exchange).then(a.symbol.cmp(&b.symbol)));
        SymbolStateResponse {
            version: "v1",
            domain: "strategy_symbol_state",
            symbol,
            exchange: exchange_filter,
            generated_at_ms: now,
            states: rows,
        }
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
        state.latest_price = Some(tick.mark.unwrap_or(mid));
        state.updated_at_ms = tick.ts_ms;
    }

    async fn update_funding(&self, tick: &FundingRateTick) {
        let mut guard = self.inner.write().await;
        let state = guard.symbol_state(tick.exchange, &tick.symbol);
        state.funding_rate = Some(tick.funding_rate);
        if let Some(mark) = tick.mark_price {
            state.latest_price = Some(mark);
        }
        state.updated_at_ms = tick.ts_ms;
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
        state.updated_at_ms = tick.ts_ms;
    }

    async fn update_trade(&self, tick: &TradeTick) {
        let signed = signed_notional(tick.side, tick.price, tick.qty);
        if !signed.is_finite() || signed == 0.0 {
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
        });
        while target.len() > MAX_FLOW_EVENTS_PER_SIDE {
            target.pop_front();
        }
        state.latest_price = Some(tick.price);
        state.updated_at_ms = tick.ts_ms;
        state.prune(tick.ts_ms);
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
        }
        state.latest_price = Some(tick.price);
        state.updated_at_ms = tick.ts_ms;
        state.prune(tick.ts_ms);
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
                });
            }
        }
        state.previous_book_top = current_top;
        state.latest_book = Some(book_snapshot(tick));
        if let Some(book) = &state.latest_book {
            state.latest_price = mid_opt(book.best_bid, book.best_ask).or(state.latest_price);
        }
        state.updated_at_ms = tick.ts_ms;
        state.prune(tick.ts_ms);
    }
}

impl StrategyStateInner {
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
    fn prune(&mut self, now: u64) {
        prune_flow(&mut self.spot_flow, now, FLOW_WINDOW_MS);
        prune_flow(&mut self.perp_flow, now, FLOW_WINDOW_MS);
        prune_flow(&mut self.ofi_samples, now, FLOW_WINDOW_MS);
        while self
            .liquidations
            .front()
            .is_some_and(|sample| now.saturating_sub(sample.ts_ms) > LIQUIDATION_WINDOW_MS)
        {
            self.liquidations.pop_front();
        }
    }

    fn snapshot(&self, now: u64) -> StrategySymbolState {
        let metrics = self.metrics();
        let long_squeeze = long_squeeze_state(&metrics);
        let short_exhaustion = short_exhaustion_state(&metrics);
        let risk_context = risk_context(self.latest_price, &long_squeeze, &short_exhaustion);
        StrategySymbolState {
            exchange: self.exchange.clone(),
            symbol: self.symbol.clone(),
            generated_at_ms: now,
            metrics,
            long_squeeze,
            short_exhaustion,
            risk_context,
        }
    }

    fn metrics(&self) -> StrategyMetrics {
        let spot_cvd = sum_flow(&self.spot_flow);
        let perp_cvd = sum_flow(&self.perp_flow);
        let (buy_liq, sell_liq) = liquidation_totals(&self.liquidations);
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
            latest_price: self.latest_price,
            funding_rate: self.funding_rate,
            open_interest: self.open_interest,
            open_interest_value: self.open_interest_value,
            open_interest_change_pct: self.open_interest_change_pct,
            spot_cvd_notional_1m: spot_cvd,
            perp_cvd_notional_1m: perp_cvd,
            cvd_divergence,
            bid_depth_notional_10: bid_depth,
            ask_depth_notional_10: ask_depth,
            bid_ask_depth_ratio_10: ratio,
            depth_pressure_10: pressure,
            ofi_best_level_1m: sum_flow(&self.ofi_samples),
            buy_liquidation_notional_15m: buy_liq,
            sell_liquidation_notional_15m: sell_liq,
        }
    }
}

pub fn spawn_strategy_state_service(
    bus: EventBus,
    store: StrategyStateStore,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = bus.subscribe_events();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                result = rx.recv() => match result {
                    Ok(shared) => store.update_event(shared.event.as_ref()).await,
                    Err(error) => warn!(%error, "strategy state stream lagged or closed"),
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

fn sum_flow(samples: &VecDeque<FlowSample>) -> Option<f64> {
    (!samples.is_empty()).then(|| samples.iter().map(|sample| sample.signed_notional).sum())
}

fn liquidation_totals(samples: &VecDeque<LiquidationSample>) -> (Option<f64>, Option<f64>) {
    if samples.is_empty() {
        return (None, None);
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
    ((buy > 0.0).then_some(buy), (sell > 0.0).then_some(sell))
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
}
