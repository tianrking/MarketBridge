use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info};

use crate::config::AppConfig;
use crate::types::{BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, now_ms};

const BOOK_SIGNAL_NOTIONAL_USDT: f64 = 1_000.0;

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
    taker_fee_bps: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy)]
struct ProfitBreakdown {
    gross: f64,
    gross_bps: f64,
    buy_fee: f64,
    sell_fee: f64,
    slip: f64,
    net: f64,
    net_bps: f64,
    fee_bps_total: f64,
    slippage_bps_total: f64,
}

impl SpreadAggregator {
    pub fn from_config(cfg: &AppConfig) -> Self {
        let mut taker_fee_bps = HashMap::new();
        for ex in cfg.enabled_exchanges() {
            if let Some(v) = cfg.taker_bps(&ex) {
                taker_fee_bps.insert(ex, v);
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
            taker_fee_bps,
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
                    format!("{}:{}msg/s", ex, hz)
                })
                .collect::<Vec<_>>()
                .join(" | ");

            let buy_fee_bps = self.fee_bps(buy_ex);
            let sell_fee_bps = self.fee_bps(sell_ex);
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
                best_book_cross_pair(&active, BOOK_SIGNAL_NOTIONAL_USDT)
            else {
                self.book_signal_started_at.remove(&key);
                continue;
            };

            let buy_fee_bps = self.fee_bps(buy_ex);
            let sell_fee_bps = self.fee_bps(sell_ex);
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
                notional_usdt = BOOK_SIGNAL_NOTIONAL_USDT,
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
                slippage_bps_total = p.slippage_bps_total,
                net = p.net,
                net_bps = p.net_bps,
                state,
                levels = %levels,
                "book_signal"
            );
        }
    }

    fn fee_bps(&self, ex: &str) -> f64 {
        self.taker_fee_bps.get(ex).copied().unwrap_or(0.0)
    }
}

fn normalize_symbol(symbol: &str, market: MarketKind) -> Box<str> {
    let mut out = String::with_capacity(symbol.len());
    for c in symbol.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        }
    }
    if market == MarketKind::Perp {
        out.push_str("_PERP");
    }
    out.into_boxed_str()
}

fn best_cross_pair(
    active: &[(&'static str, &MarketTick)],
) -> Option<(&'static str, f64, &'static str, f64)> {
    let mut best_pair: Option<(&'static str, f64, &'static str, f64)> = None;
    for (buy_ex, buy_t) in active {
        for (sell_ex, sell_t) in active {
            if buy_ex == sell_ex {
                continue;
            }
            let spread = sell_t.bid - buy_t.ask;
            if best_pair.is_none_or(|(_, best_ask, _, best_bid)| spread > (best_bid - best_ask)) {
                best_pair = Some((*buy_ex, buy_t.ask, *sell_ex, sell_t.bid));
            }
        }
    }
    best_pair
}

fn best_book_cross_pair(
    active: &[(&'static str, &OrderBookTick)],
    notional: f64,
) -> Option<(&'static str, f64, &'static str, f64)> {
    let mut best_pair: Option<(&'static str, f64, &'static str, f64)> = None;
    for (buy_ex, buy_book) in active {
        let Some(buy_avg_ask) = average_execution_price(&buy_book.asks, notional) else {
            continue;
        };

        for (sell_ex, sell_book) in active {
            if buy_ex == sell_ex {
                continue;
            }
            let Some(sell_avg_bid) = average_execution_price(&sell_book.bids, notional) else {
                continue;
            };

            let spread = sell_avg_bid - buy_avg_ask;
            if best_pair.is_none_or(|(_, best_ask, _, best_bid)| spread > (best_bid - best_ask)) {
                best_pair = Some((*buy_ex, buy_avg_ask, *sell_ex, sell_avg_bid));
            }
        }
    }
    best_pair
}

fn average_execution_price(levels: &[BookLevel], target_quote_notional: f64) -> Option<f64> {
    if target_quote_notional <= 0.0 {
        return None;
    }

    let mut remaining_quote = target_quote_notional;
    let mut filled_quote = 0.0;
    let mut filled_base = 0.0;

    for level in levels {
        if level.price <= 0.0 || level.qty <= 0.0 {
            continue;
        }

        let level_quote = level.price * level.qty;
        let take_quote = remaining_quote.min(level_quote);
        filled_quote += take_quote;
        filled_base += take_quote / level.price;
        remaining_quote -= take_quote;

        if remaining_quote <= f64::EPSILON {
            break;
        }
    }

    if remaining_quote <= f64::EPSILON && filled_base > 0.0 {
        Some(filled_quote / filled_base)
    } else {
        None
    }
}

fn compute_profit(
    ask: f64,
    bid: f64,
    buy_fee_bps: f64,
    sell_fee_bps: f64,
    slippage_bps_single_leg: f64,
) -> ProfitBreakdown {
    let gross = bid - ask;
    let gross_bps = if ask > 0.0 {
        gross / ask * 10_000.0
    } else {
        0.0
    };
    let fee_bps_total = buy_fee_bps + sell_fee_bps;
    let slippage_bps_total = slippage_bps_single_leg * 2.0;
    let buy_fee = ask * buy_fee_bps / 10_000.0;
    let sell_fee = bid * sell_fee_bps / 10_000.0;
    let slip = ((ask + bid) / 2.0) * slippage_bps_total / 10_000.0;
    let net = gross - buy_fee - sell_fee - slip;
    let net_bps = if ask > 0.0 {
        (net / ask) * 10_000.0
    } else {
        0.0
    };
    ProfitBreakdown {
        gross,
        gross_bps,
        buy_fee,
        sell_fee,
        slip,
        net,
        net_bps,
        fee_bps_total,
        slippage_bps_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_cross_pair_excludes_same_exchange() {
        let t1 = MarketTick {
            exchange: "a",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bid: 101.0,
            ask: 102.0,
            mark: None,
            funding_rate: None,
            ts_ms: 1,
        };
        let t2 = MarketTick {
            exchange: "b",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bid: 110.0,
            ask: 111.0,
            mark: None,
            funding_rate: None,
            ts_ms: 1,
        };
        let active = vec![("a", &t1), ("b", &t2)];
        let (buy_ex, _, sell_ex, _) = best_cross_pair(&active).expect("pair");
        assert_ne!(buy_ex, sell_ex);
    }

    #[test]
    fn compute_profit_matches_expected_direction() {
        let p = compute_profit(80152.6, 80164.4, 10.0, 10.0, 0.5);
        assert!(p.gross > 0.0);
        assert!(p.net < 0.0);
        assert!(p.fee_bps_total > p.gross_bps);
    }

    #[test]
    fn average_execution_price_consumes_multiple_levels() {
        let levels = vec![
            BookLevel {
                price: 100.0,
                qty: 5.0,
            },
            BookLevel {
                price: 110.0,
                qty: 5.0,
            },
        ];

        let avg = average_execution_price(&levels, 1_000.0).expect("enough depth");

        assert!((avg - 104.761_904_761_904_76).abs() < 1e-9);
    }

    #[test]
    fn best_book_cross_pair_uses_depth_and_excludes_same_exchange() {
        let buy_book = OrderBookTick {
            exchange: "a",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bids: vec![BookLevel {
                price: 99.0,
                qty: 10.0,
            }],
            asks: vec![
                BookLevel {
                    price: 100.0,
                    qty: 1.0,
                },
                BookLevel {
                    price: 110.0,
                    qty: 10.0,
                },
            ],
            last_update_id: None,
            ts_ms: 1,
        };
        let sell_book = OrderBookTick {
            exchange: "b",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bids: vec![
                BookLevel {
                    price: 120.0,
                    qty: 1.0,
                },
                BookLevel {
                    price: 115.0,
                    qty: 10.0,
                },
            ],
            asks: vec![BookLevel {
                price: 121.0,
                qty: 10.0,
            }],
            last_update_id: None,
            ts_ms: 1,
        };
        let active = vec![("a", &buy_book), ("b", &sell_book)];

        let (buy_ex, buy_avg, sell_ex, sell_avg) =
            best_book_cross_pair(&active, 1_000.0).expect("book pair");

        assert_eq!(buy_ex, "a");
        assert_eq!(sell_ex, "b");
        assert!(buy_avg > 100.0);
        assert!(sell_avg < 120.0);
    }
}
