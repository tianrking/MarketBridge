use crate::types::{BookLevel, MarketKind, MarketTick, OrderBookTick, TradeSide};

#[derive(Debug, Clone, Copy)]
pub struct ProfitBreakdown {
    pub gross: f64,
    pub gross_bps: f64,
    pub buy_fee: f64,
    pub sell_fee: f64,
    pub slip: f64,
    pub net: f64,
    pub net_bps: f64,
    pub fee_bps_total: f64,
    pub slippage_bps_total: f64,
}

pub fn normalize_symbol(symbol: &str, market: MarketKind) -> Box<str> {
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

pub fn best_cross_pair(
    active: &[(&'static str, &MarketTick)],
    fees: impl Fn(&str, &str) -> (f64, f64),
    slippage_bps: f64,
) -> Option<(&'static str, f64, &'static str, f64)> {
    let mut best_pair: Option<(&'static str, f64, &'static str, f64)> = None;
    let mut best_net = f64::NEG_INFINITY;
    for (buy_ex, buy_t) in active {
        for (sell_ex, sell_t) in active {
            if buy_ex == sell_ex
                || buy_t.market != sell_t.market
                || normalize_symbol(&buy_t.symbol, buy_t.market)
                    != normalize_symbol(&sell_t.symbol, sell_t.market)
                || !valid_bbo(buy_t.bid, buy_t.ask)
                || !valid_bbo(sell_t.bid, sell_t.ask)
            {
                continue;
            }
            let (buy_fee, sell_fee) = fees(buy_ex, sell_ex);
            let net =
                compute_profit(buy_t.ask, sell_t.bid, buy_fee, sell_fee, slippage_bps).net_bps;
            if net.is_finite() && net > best_net {
                best_net = net;
                best_pair = Some((*buy_ex, buy_t.ask, *sell_ex, sell_t.bid));
            }
        }
    }
    best_pair
}

pub fn best_book_cross_pair(
    active: &[(&'static str, &OrderBookTick)],
    notional: f64,
    fees: impl Fn(&str, &str) -> (f64, f64),
    slippage_bps: f64,
) -> Option<(&'static str, f64, &'static str, f64, f64)> {
    let mut best_pair = None;
    let mut best_net = f64::NEG_INFINITY;
    for (buy_ex, buy_book) in active {
        if !valid_book(buy_book) {
            continue;
        }
        let Some(buy_avg_ask) = average_execution_price(&buy_book.asks, notional) else {
            continue;
        };
        let base_quantity = notional / buy_avg_ask;

        for (sell_ex, sell_book) in active {
            if buy_ex == sell_ex
                || !valid_book(sell_book)
                || buy_book.market != sell_book.market
                || normalize_symbol(&buy_book.symbol, buy_book.market)
                    != normalize_symbol(&sell_book.symbol, sell_book.market)
            {
                continue;
            }
            let Some(sell_quote) =
                crate::research_engine::quote_for_base(&sell_book.bids, base_quantity)
            else {
                continue;
            };
            let sell_avg_bid = sell_quote / base_quantity;
            let (buy_fee, sell_fee) = fees(buy_ex, sell_ex);
            let net = compute_profit(buy_avg_ask, sell_avg_bid, buy_fee, sell_fee, slippage_bps)
                .net
                * base_quantity;
            if net.is_finite() && net > best_net {
                best_net = net;
                best_pair = Some((*buy_ex, buy_avg_ask, *sell_ex, sell_avg_bid, base_quantity));
            }
        }
    }
    best_pair
}

pub fn average_execution_price(levels: &[BookLevel], target_quote_notional: f64) -> Option<f64> {
    if !target_quote_notional.is_finite() || target_quote_notional <= 0.0 {
        return None;
    }

    let mut remaining_quote = target_quote_notional;
    let mut filled_quote = 0.0;
    let mut filled_base = 0.0;

    for level in levels {
        if !level.price.is_finite()
            || !level.qty.is_finite()
            || level.price <= 0.0
            || level.qty <= 0.0
        {
            return None;
        }

        let level_quote = level.price * level.qty;
        let take_quote = remaining_quote.min(level_quote);
        filled_quote += take_quote;
        filled_base += take_quote / level.price;
        remaining_quote -= take_quote;

        if remaining_quote <= target_quote_notional * 1e-12 {
            break;
        }
    }

    if remaining_quote <= target_quote_notional * 1e-12
        && filled_base > 0.0
        && filled_quote.is_finite()
    {
        Some(filled_quote / filled_base)
    } else {
        None
    }
}

fn valid_bbo(bid: f64, ask: f64) -> bool {
    bid.is_finite() && ask.is_finite() && bid > 0.0 && ask >= bid
}

fn valid_book(book: &OrderBookTick) -> bool {
    book.bids
        .first()
        .zip(book.asks.first())
        .is_some_and(|(b, a)| valid_bbo(b.price, a.price))
        && book.bids.windows(2).all(|w| w[0].price > w[1].price)
        && book.asks.windows(2).all(|w| w[0].price < w[1].price)
}

pub fn depth_pressure(book: &OrderBookTick, levels: usize) -> Option<f64> {
    let levels = levels.max(1);
    let bid_notional = book
        .bids
        .iter()
        .take(levels)
        .filter(|level| level.price > 0.0 && level.qty > 0.0)
        .map(|level| level.price * level.qty)
        .sum::<f64>();
    let ask_notional = book
        .asks
        .iter()
        .take(levels)
        .filter(|level| level.price > 0.0 && level.qty > 0.0)
        .map(|level| level.price * level.qty)
        .sum::<f64>();
    let total = bid_notional + ask_notional;
    if total > 0.0 {
        Some((bid_notional - ask_notional) / total)
    } else {
        None
    }
}

pub fn signed_notional(side: TradeSide, price: f64, qty: f64) -> f64 {
    let notional = price * qty;
    match side {
        TradeSide::Buy => notional,
        TradeSide::Sell => -notional,
        TradeSide::Unknown => 0.0,
    }
}

pub fn compute_profit(
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
    fn ranks_net_edges_instead_of_largest_raw_spread() {
        let a = MarketTick {
            exchange: "a",
            market: MarketKind::Spot,
            symbol: "BTCUSD".into(),
            bid: 99.0,
            ask: 100.0,
            mark: None,
            funding_rate: None,
            ts_ms: 1,
        };
        let mut b = a.clone();
        b.exchange = "b";
        b.bid = 110.0;
        b.ask = 111.0;
        let mut c = a.clone();
        c.exchange = "c";
        c.bid = 105.0;
        c.ask = 106.0;
        let pair = best_cross_pair(
            &[("a", &a), ("b", &b), ("c", &c)],
            |_, sell| (0.0, if sell == "b" { 1000.0 } else { 0.0 }),
            0.0,
        )
        .unwrap();
        assert_eq!((pair.0, pair.2), ("a", "c"));
    }

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
        let (buy_ex, _, sell_ex, _) =
            best_cross_pair(&active, |_, _| (0.0, 0.0), 0.0).expect("pair");
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

        let (buy_ex, buy_avg, sell_ex, sell_avg, quantity) =
            best_book_cross_pair(&active, 1_000.0, |_, _| (0.0, 0.0), 0.0).expect("book pair");

        assert_eq!(buy_ex, "a");
        assert_eq!(sell_ex, "b");
        assert!(buy_avg > 100.0);
        assert!(sell_avg < 120.0);
        assert!((quantity - (1.0 + 900.0 / 110.0)).abs() < 1e-9);
        assert!((sell_avg * quantity - (120.0 + 900.0 / 110.0 * 115.0)).abs() < 1e-9);
    }

    #[test]
    fn depth_pressure_reports_bid_ask_imbalance() {
        let book = OrderBookTick {
            exchange: "a",
            market: MarketKind::Perp,
            symbol: "BTCUSDT".into(),
            bids: vec![BookLevel {
                price: 100.0,
                qty: 2.0,
            }],
            asks: vec![BookLevel {
                price: 100.0,
                qty: 1.0,
            }],
            last_update_id: None,
            ts_ms: 1,
        };

        let pressure = depth_pressure(&book, 5).expect("pressure");

        assert!((pressure - (1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn signed_notional_uses_trade_side() {
        assert_eq!(signed_notional(TradeSide::Buy, 100.0, 2.0), 200.0);
        assert_eq!(signed_notional(TradeSide::Sell, 100.0, 2.0), -200.0);
        assert_eq!(signed_notional(TradeSide::Unknown, 100.0, 2.0), 0.0);
    }
}
