use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info};

use crate::aggregator_signal::{
    best_book_cross_pair, best_cross_pair, compute_profit, normalize_symbol,
};
use crate::config::{AppConfig, StrategyFeeMode};
use crate::types::{DataEvent, MarketTick, OrderBookTick, now_ms};

pub struct SpreadAggregator {
    books: HashMap<Box<str>, HashMap<&'static str, MarketTick>>,
    order_books: HashMap<Box<str>, HashMap<&'static str, OrderBookTick>>,
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

    pub async fn run(mut self, mut rx: mpsc::Receiver<DataEvent>) {
        let mut report_tick = interval(self.report_interval);

        loop {
            tokio::select! {
                _ = report_tick.tick() => self.report_once(),
                maybe = rx.recv() => {
                    match maybe {
                        Some(DataEvent::Tick(t)) => self.on_tick(t),
                        Some(DataEvent::OrderBook(book)) => self.on_order_book(book),
                        Some(DataEvent::Heartbeat { exchange, ts_ms }) => {
                            debug!(exchange, ts_ms, "heartbeat");
                        }
                        Some(
                            DataEvent::FundingRate(_)
                            | DataEvent::OpenInterest(_)
                            | DataEvent::Trade(_)
                            | DataEvent::Liquidation(_)
                            | DataEvent::ExternalSignal(_),
                        ) => {}
                        None => break,
                    }
                }
            }
        }
    }

    fn on_tick(&mut self, tick: MarketTick) {
        let key = normalize_symbol(&tick.symbol, tick.market);
        let ex = tick.exchange;
        self.books.entry(key.clone()).or_default().insert(ex, tick);
        *self
            .tick_counts
            .entry(key)
            .or_default()
            .entry(ex)
            .or_default() += 1;
    }

    fn on_order_book(&mut self, book: OrderBookTick) {
        let key = normalize_symbol(&book.symbol, book.market);
        self.order_books
            .entry(key)
            .or_default()
            .insert(book.exchange, book);
    }

    fn report_once(&mut self) {
        let now = now_ms();
        let secs = self.report_interval.as_secs_f64();

        self.report_bbo_spreads(now, secs);
        self.report_book_spreads(now);
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
            let count_snapshot = self.tick_counts.get(&key).cloned().unwrap_or_default();

            let legs = active
                .iter()
                .map(|(ex, t)| format!("{} b:{:.2} a:{:.2}", ex, t.bid, t.ask))
                .collect::<Vec<_>>()
                .join(" | ");

            let freq = active
                .iter()
                .map(|(ex, _)| {
                    let c = *count_snapshot.get(ex).unwrap_or(&0);
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
