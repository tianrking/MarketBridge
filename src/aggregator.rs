use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info};

use crate::aggregator_signal::{
    best_book_cross_pair, best_cross_pair, compute_profit, depth_pressure, normalize_symbol,
    signed_notional,
};
use crate::config::{AppConfig, StrategyFeeMode};
use crate::types::{
    DataEvent, FundingRateTick, LiquidationTick, MarketTick, OpenInterestTick, OrderBookTick,
    TradeTick, now_ms,
};

const TRADE_FLOW_WINDOW_MS: u64 = 60_000;
const LIQUIDATION_WINDOW_MS: u64 = 60_000;
const DEPTH_PRESSURE_LEVELS: usize = 5;
const MAX_FLOW_WINDOW_POINTS: usize = 20_000;

pub struct SpreadAggregator {
    books: HashMap<Box<str>, HashMap<&'static str, MarketTick>>,
    order_books: HashMap<Box<str>, HashMap<&'static str, OrderBookTick>>,
    funding: HashMap<Box<str>, HashMap<&'static str, FundingRateTick>>,
    open_interest: HashMap<Box<str>, HashMap<&'static str, OpenInterestState>>,
    trade_flow: HashMap<Box<str>, FlowWindow>,
    liquidation_flow: HashMap<Box<str>, FlowWindow>,
    tick_counts: HashMap<Box<str>, HashMap<&'static str, u64>>,
    signal_started_at: HashMap<Box<str>, u64>,
    book_signal_started_at: HashMap<Box<str>, u64>,
    report_interval: Duration,
    stale_ttl_ms: u64,
    min_profit_usdt: f64,
    min_profit_bps: f64,
    min_signal_hold_ms: u64,
    slippage_bps: f64,
    book_signal_notional_usdt: f64,
    fee_mode: StrategyFeeMode,
    maker_fee_bps: HashMap<String, f64>,
    taker_fee_bps: HashMap<String, f64>,
    fallback_maker_fee_bps: f64,
    fallback_taker_fee_bps: f64,
}

impl SpreadAggregator {
    pub fn from_config(cfg: &AppConfig) -> Self {
        let mut taker_fee_bps = HashMap::new();
        let mut maker_fee_bps = HashMap::new();
        for ex in cfg.enabled_exchanges() {
            if let Some(v) = cfg.taker_bps(&ex) {
                taker_fee_bps.insert(ex.clone(), v);
            }
            if let Some(v) = cfg.maker_bps(&ex) {
                maker_fee_bps.insert(ex, v);
            }
        }

        Self {
            books: HashMap::new(),
            order_books: HashMap::new(),
            funding: HashMap::new(),
            open_interest: HashMap::new(),
            trade_flow: HashMap::new(),
            liquidation_flow: HashMap::new(),
            tick_counts: HashMap::new(),
            signal_started_at: HashMap::new(),
            book_signal_started_at: HashMap::new(),
            report_interval: Duration::from_millis(cfg.runtime.report_interval_ms.max(100)),
            stale_ttl_ms: cfg.runtime.stale_ttl_ms,
            min_profit_usdt: cfg.strategy.min_profit_usdt,
            min_profit_bps: cfg.strategy.min_profit_bps,
            min_signal_hold_ms: cfg.strategy.min_signal_hold_ms,
            slippage_bps: cfg.strategy.slippage_bps,
            book_signal_notional_usdt: cfg.strategy.book_signal_notional_usdt,
            fee_mode: cfg.strategy.fee_mode,
            maker_fee_bps,
            taker_fee_bps,
            fallback_maker_fee_bps: cfg.strategy.fallback_maker_fee_bps,
            fallback_taker_fee_bps: cfg.strategy.fallback_taker_fee_bps,
        }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Arc<DataEvent>>) {
        let mut report_tick = interval(self.report_interval);

        loop {
            tokio::select! {
                _ = report_tick.tick() => self.report_once(),
                maybe = rx.recv() => {
                    match maybe.as_deref() {
                        Some(DataEvent::Tick(t)) => self.on_tick(t),
                        Some(DataEvent::OrderBook(book)) => self.on_order_book(book),
                        Some(DataEvent::FundingRate(funding)) => self.on_funding(funding),
                        Some(DataEvent::OpenInterest(open_interest)) => {
                            self.on_open_interest(open_interest)
                        }
                        Some(DataEvent::Trade(trade)) => self.on_trade(trade),
                        Some(DataEvent::Liquidation(liquidation)) => {
                            self.on_liquidation(liquidation)
                        }
                        Some(DataEvent::Heartbeat { exchange, ts_ms }) => {
                            debug!(exchange, ts_ms, "heartbeat");
                        }
                        Some(DataEvent::ExternalSignal(_)) => {}
                        None => break,
                    }
                }
            }
        }
    }

    fn on_tick(&mut self, tick: &MarketTick) {
        let key = normalize_symbol(&tick.symbol, tick.market);
        let ex = tick.exchange;
        self.books
            .entry(key.clone())
            .or_default()
            .insert(ex, tick.clone());
        *self
            .tick_counts
            .entry(key)
            .or_default()
            .entry(ex)
            .or_default() += 1;
    }

    fn on_order_book(&mut self, book: &OrderBookTick) {
        let key = normalize_symbol(&book.symbol, book.market);
        self.order_books
            .entry(key)
            .or_default()
            .insert(book.exchange, book.clone());
    }

    fn on_funding(&mut self, tick: &FundingRateTick) {
        let key = normalize_symbol(&tick.symbol, crate::types::MarketKind::Perp);
        self.funding
            .entry(key)
            .or_default()
            .insert(tick.exchange, tick.clone());
    }

    fn on_open_interest(&mut self, tick: &OpenInterestTick) {
        let key = normalize_symbol(&tick.symbol, crate::types::MarketKind::Perp);
        let by_exchange = self.open_interest.entry(key).or_default();
        let previous = by_exchange
            .get(tick.exchange)
            .map(|state| state.current.clone());
        by_exchange.insert(
            tick.exchange,
            OpenInterestState {
                previous,
                current: tick.clone(),
            },
        );
    }

    fn on_trade(&mut self, trade: &TradeTick) {
        let key = normalize_symbol(&trade.symbol, trade.market);
        self.trade_flow.entry(key).or_default().add(
            trade.ts_ms,
            signed_notional(trade.side, trade.price, trade.qty),
        );
    }

    fn on_liquidation(&mut self, liquidation: &LiquidationTick) {
        let key = normalize_symbol(&liquidation.symbol, crate::types::MarketKind::Perp);
        self.liquidation_flow.entry(key).or_default().add(
            liquidation.ts_ms,
            signed_notional(liquidation.side, liquidation.price, liquidation.qty),
        );
    }

    fn report_once(&mut self) {
        let now = now_ms();
        let secs = self.report_interval.as_secs_f64();

        self.report_bbo_spreads(now, secs);
        self.report_book_spreads(now);
        self.report_funding_divergence(now);
        self.report_open_interest_changes(now);
        self.report_flow_windows(now);
        self.report_depth_pressure(now);
    }

    fn report_bbo_spreads(&mut self, now: u64, secs: f64) {
        let keys: Vec<Box<str>> = self.books.keys().cloned().collect();
        for key in keys {
            let Some(by_exchange) = self.books.get(&key) else {
                continue;
            };

            let mut active: Vec<(&'static str, &MarketTick)> = by_exchange
                .iter()
                .filter(|(_, t)| now.saturating_sub(t.ts_ms) <= self.stale_ttl_ms)
                .map(|(ex, t)| (*ex, t))
                .collect();

            if active.len() < 2 {
                self.signal_started_at.remove(&key);
                continue;
            }
            active.sort_by_key(|(ex, _)| *ex);

            // Find best cross-exchange pair only (buy_ex != sell_ex).
            let best_pair = best_cross_pair(&active);

            let Some((buy_ex, ask, sell_ex, bid)) = best_pair else {
                continue;
            };

            let symbol = key.as_ref();
            let market = active[0].1.market;
            let count_snapshot = self.tick_counts.get(&key);

            let legs = active
                .iter()
                .map(|(ex, t)| format!("{} b:{:.2} a:{:.2}", ex, t.bid, t.ask))
                .collect::<Vec<_>>()
                .join(" | ");

            let freq = active
                .iter()
                .map(|(ex, _)| {
                    let c = count_snapshot
                        .and_then(|counts| counts.get(ex))
                        .copied()
                        .unwrap_or(0);
                    let hz = (c as f64 / secs).round() as u64;
                    format!("{ex}:{hz}msg/s")
                })
                .collect::<Vec<_>>()
                .join(" | ");

            let (buy_fee_bps, sell_fee_bps) = self.leg_fee_bps(buy_ex, sell_ex);
            let p = compute_profit(ask, bid, buy_fee_bps, sell_fee_bps, self.slippage_bps);

            let eligible = p.net >= self.min_profit_usdt && p.net_bps >= self.min_profit_bps;
            let state = if eligible {
                let started = self.signal_started_at.entry(key.clone()).or_insert(now);
                if now.saturating_sub(*started) >= self.min_signal_hold_ms {
                    "TRIGGER"
                } else {
                    "HOLDING"
                }
            } else {
                self.signal_started_at.remove(&key);
                "FILTERED"
            };

            let mark = active.iter().find_map(|(_, t)| t.mark);
            let funding = active.iter().find_map(|(_, t)| t.funding_rate);
            info!(
                symbol,
                market = ?market,
                buy_ex,
                buy_ask = ask,
                sell_ex,
                sell_bid = bid,
                mark,
                funding_rate = funding,
                gross = p.gross,
                gross_bps = p.gross_bps,
                buy_fee = p.buy_fee,
                sell_fee = p.sell_fee,
                slip = p.slip,
                fee_bps_total = p.fee_bps_total,
                fee_mode = ?self.fee_mode,
                slippage_bps_total = p.slippage_bps_total,
                net = p.net,
                net_bps = p.net_bps,
                state,
                legs = %legs,
                freq = %freq,
                "signal"
            );

            if let Some(counts) = self.tick_counts.get_mut(&key) {
                for c in counts.values_mut() {
                    *c = 0;
                }
            }
        }
    }

    fn report_book_spreads(&mut self, now: u64) {
        let keys: Vec<Box<str>> = self.order_books.keys().cloned().collect();
        for key in keys {
            let Some(by_exchange) = self.order_books.get(&key) else {
                continue;
            };

            let mut active: Vec<(&'static str, &OrderBookTick)> = by_exchange
                .iter()
                .filter(|(_, book)| now.saturating_sub(book.ts_ms) <= self.stale_ttl_ms)
                .map(|(ex, book)| (*ex, book))
                .collect();

            if active.len() < 2 {
                self.book_signal_started_at.remove(&key);
                continue;
            }
            active.sort_by_key(|(ex, _)| *ex);

            let Some((buy_ex, buy_avg_ask, sell_ex, sell_avg_bid)) =
                best_book_cross_pair(&active, self.book_signal_notional_usdt)
            else {
                self.book_signal_started_at.remove(&key);
                continue;
            };

            let (buy_fee_bps, sell_fee_bps) = self.leg_fee_bps(buy_ex, sell_ex);
            let p = compute_profit(
                buy_avg_ask,
                sell_avg_bid,
                buy_fee_bps,
                sell_fee_bps,
                self.slippage_bps,
            );

            let eligible = p.net >= self.min_profit_usdt && p.net_bps >= self.min_profit_bps;
            let state = if eligible {
                let started = self
                    .book_signal_started_at
                    .entry(key.clone())
                    .or_insert(now);
                if now.saturating_sub(*started) >= self.min_signal_hold_ms {
                    "TRIGGER"
                } else {
                    "HOLDING"
                }
            } else {
                self.book_signal_started_at.remove(&key);
                "FILTERED"
            };

            let levels = active
                .iter()
                .map(|(ex, book)| {
                    format!("{} bids:{} asks:{}", ex, book.bids.len(), book.asks.len())
                })
                .collect::<Vec<_>>()
                .join(" | ");

            info!(
                symbol = key.as_ref(),
                market = ?active[0].1.market,
                notional_usdt = self.book_signal_notional_usdt,
                buy_ex,
                buy_avg_ask,
                sell_ex,
                sell_avg_bid,
                gross = p.gross,
                gross_bps = p.gross_bps,
                buy_fee = p.buy_fee,
                sell_fee = p.sell_fee,
                slip = p.slip,
                fee_bps_total = p.fee_bps_total,
                fee_mode = ?self.fee_mode,
                slippage_bps_total = p.slippage_bps_total,
                net = p.net,
                net_bps = p.net_bps,
                state,
                levels = %levels,
                "book_signal"
            );
        }
    }

    fn report_funding_divergence(&mut self, now: u64) {
        for (key, by_exchange) in &self.funding {
            let mut active = by_exchange
                .iter()
                .filter(|(_, tick)| now.saturating_sub(tick.ts_ms) <= self.stale_ttl_ms)
                .map(|(exchange, tick)| (*exchange, tick.funding_rate))
                .collect::<Vec<_>>();
            if active.len() < 2 {
                continue;
            }
            active.sort_by(|a, b| a.1.total_cmp(&b.1));
            let (min_ex, min_rate) = active[0];
            let (max_ex, max_rate) = active[active.len() - 1];
            let spread = max_rate - min_rate;
            info!(
                symbol = key.as_ref(),
                min_ex,
                min_rate,
                max_ex,
                max_rate,
                spread,
                spread_bps = spread * 10_000.0,
                venues = active.len(),
                "funding_divergence_signal"
            );
        }
    }

    fn report_open_interest_changes(&mut self, now: u64) {
        for (key, by_exchange) in &self.open_interest {
            for (exchange, state) in by_exchange {
                let current = &state.current;
                if now.saturating_sub(current.ts_ms) > self.stale_ttl_ms {
                    continue;
                }
                let Some(previous) = &state.previous else {
                    continue;
                };
                if previous.open_interest <= 0.0 {
                    continue;
                }
                let delta = current.open_interest - previous.open_interest;
                let delta_pct = delta / previous.open_interest;
                info!(
                    symbol = key.as_ref(),
                    exchange,
                    open_interest = current.open_interest,
                    previous_open_interest = previous.open_interest,
                    delta,
                    delta_pct,
                    open_interest_value = current.open_interest_value,
                    "open_interest_change_signal"
                );
            }
        }
    }

    fn report_flow_windows(&mut self, now: u64) {
        for (key, window) in &mut self.trade_flow {
            window.prune(now, TRADE_FLOW_WINDOW_MS);
            let summary = window.summary();
            if summary.count == 0 {
                continue;
            }
            info!(
                symbol = key.as_ref(),
                window_ms = TRADE_FLOW_WINDOW_MS,
                signed_notional = summary.signed_notional,
                absolute_notional = summary.absolute_notional,
                imbalance = summary.imbalance,
                count = summary.count,
                "trade_imbalance_signal"
            );
        }

        for (key, window) in &mut self.liquidation_flow {
            window.prune(now, LIQUIDATION_WINDOW_MS);
            let summary = window.summary();
            if summary.count == 0 {
                continue;
            }
            info!(
                symbol = key.as_ref(),
                window_ms = LIQUIDATION_WINDOW_MS,
                signed_notional = summary.signed_notional,
                absolute_notional = summary.absolute_notional,
                imbalance = summary.imbalance,
                count = summary.count,
                "liquidation_burst_signal"
            );
        }
    }

    fn report_depth_pressure(&mut self, now: u64) {
        for (key, by_exchange) in &self.order_books {
            for (exchange, book) in by_exchange {
                if now.saturating_sub(book.ts_ms) > self.stale_ttl_ms {
                    continue;
                }
                let Some(pressure) = depth_pressure(book, DEPTH_PRESSURE_LEVELS) else {
                    continue;
                };
                info!(
                    symbol = key.as_ref(),
                    exchange,
                    levels = DEPTH_PRESSURE_LEVELS,
                    pressure,
                    "depth_pressure_signal"
                );
            }
        }
    }

    fn leg_fee_bps(&self, buy_ex: &str, sell_ex: &str) -> (f64, f64) {
        match self.fee_mode {
            StrategyFeeMode::Taker => (self.taker_bps(buy_ex), self.taker_bps(sell_ex)),
            StrategyFeeMode::Maker => (self.maker_bps(buy_ex), self.maker_bps(sell_ex)),
            StrategyFeeMode::MakerBuyTakerSell => (self.maker_bps(buy_ex), self.taker_bps(sell_ex)),
            StrategyFeeMode::TakerBuyMakerSell => (self.taker_bps(buy_ex), self.maker_bps(sell_ex)),
        }
    }

    fn taker_bps(&self, ex: &str) -> f64 {
        self.taker_fee_bps
            .get(ex)
            .copied()
            .unwrap_or(self.fallback_taker_fee_bps)
    }

    fn maker_bps(&self, ex: &str) -> f64 {
        self.maker_fee_bps
            .get(ex)
            .copied()
            .unwrap_or(self.fallback_maker_fee_bps)
    }
}

#[derive(Debug, Clone)]
struct OpenInterestState {
    previous: Option<OpenInterestTick>,
    current: OpenInterestTick,
}

#[derive(Debug, Clone, Default)]
struct FlowWindow {
    rows: Vec<FlowPoint>,
}

impl FlowWindow {
    fn add(&mut self, ts_ms: u64, signed_notional: f64) {
        self.rows.push(FlowPoint {
            ts_ms,
            signed_notional,
        });
        if self.rows.len() > MAX_FLOW_WINDOW_POINTS {
            let excess = self.rows.len() - MAX_FLOW_WINDOW_POINTS;
            self.rows.drain(0..excess);
        }
    }

    fn prune(&mut self, now: u64, window_ms: u64) {
        self.rows
            .retain(|row| now.saturating_sub(row.ts_ms) <= window_ms);
    }

    fn summary(&self) -> FlowSummary {
        let signed_notional = self.rows.iter().map(|row| row.signed_notional).sum::<f64>();
        let absolute_notional = self
            .rows
            .iter()
            .map(|row| row.signed_notional.abs())
            .sum::<f64>();
        let imbalance = if absolute_notional > 0.0 {
            signed_notional / absolute_notional
        } else {
            0.0
        };
        FlowSummary {
            signed_notional,
            absolute_notional,
            imbalance,
            count: self.rows.len(),
        }
    }
}

#[derive(Debug, Clone)]
struct FlowPoint {
    ts_ms: u64,
    signed_notional: f64,
}

#[derive(Debug, Clone, Copy)]
struct FlowSummary {
    signed_notional: f64,
    absolute_notional: f64,
    imbalance: f64,
    count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_window_prunes_and_summarizes_signed_notional() {
        let mut window = FlowWindow::default();
        window.add(1_000, 100.0);
        window.add(1_100, -40.0);
        window.add(10, 999.0);

        window.prune(1_200, 500);
        let summary = window.summary();

        assert_eq!(summary.count, 2);
        assert_eq!(summary.signed_notional, 60.0);
        assert_eq!(summary.absolute_notional, 140.0);
        assert!((summary.imbalance - (60.0 / 140.0)).abs() < 1e-9);
    }

    #[test]
    fn flow_window_caps_retained_points() {
        let mut window = FlowWindow::default();
        for i in 0..(MAX_FLOW_WINDOW_POINTS + 10) {
            window.add(i as u64, 1.0);
        }

        assert_eq!(window.rows.len(), MAX_FLOW_WINDOW_POINTS);
        assert_eq!(window.rows.first().expect("first").ts_ms, 10);
    }
}
