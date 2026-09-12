use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketKind {
    Spot,
    Perp,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketTick {
    pub exchange: &'static str,
    pub market: MarketKind,
    pub symbol: Box<str>,
    pub bid: f64,
    pub ask: f64,
    pub mark: Option<f64>,
    pub funding_rate: Option<f64>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FundingRateTick {
    pub exchange: &'static str,
    pub symbol: Box<str>,
    pub funding_rate: f64,
    pub next_funding_time_ms: Option<u64>,
    pub mark_price: Option<f64>,
    pub index_price: Option<f64>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenInterestTick {
    pub exchange: &'static str,
    pub symbol: Box<str>,
    pub open_interest: f64,
    pub open_interest_value: Option<f64>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    Buy,
    Sell,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeTick {
    pub exchange: &'static str,
    pub market: MarketKind,
    pub symbol: Box<str>,
    pub price: f64,
    pub qty: f64,
    pub side: TradeSide,
    pub trade_id: Option<Box<str>>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiquidationTick {
    pub exchange: &'static str,
    pub symbol: Box<str>,
    pub side: TradeSide,
    pub price: f64,
    pub qty: f64,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookLevel {
    pub price: f64,
    pub qty: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderBookTick {
    pub exchange: &'static str,
    pub market: MarketKind,
    pub symbol: Box<str>,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub last_update_id: Option<u64>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalSignalTick {
    pub source: &'static str,
    pub category: Box<str>,
    pub symbol: Option<Box<str>>,
    pub metric: Box<str>,
    pub value: Option<f64>,
    pub score: Option<f64>,
    pub title: Option<Box<str>>,
    pub url: Option<Box<str>>,
    pub ts_ms: u64,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataEvent {
    Tick(MarketTick),
    FundingRate(FundingRateTick),
    OpenInterest(OpenInterestTick),
    Trade(TradeTick),
    Liquidation(LiquidationTick),
    OrderBook(OrderBookTick),
    ExternalSignal(ExternalSignalTick),
    Heartbeat { exchange: &'static str, ts_ms: u64 },
}

impl DataEvent {
    pub fn has_finite_numbers(&self) -> bool {
        match self {
            DataEvent::Tick(t) => {
                finite(t.bid)
                    && finite(t.ask)
                    && t.mark.is_none_or(finite)
                    && t.funding_rate.is_none_or(finite)
            }
            DataEvent::FundingRate(t) => {
                finite(t.funding_rate)
                    && t.mark_price.is_none_or(finite)
                    && t.index_price.is_none_or(finite)
            }
            DataEvent::OpenInterest(t) => {
                finite(t.open_interest) && t.open_interest_value.is_none_or(finite)
            }
            DataEvent::Trade(t) => finite(t.price) && finite(t.qty),
            DataEvent::Liquidation(t) => finite(t.price) && finite(t.qty),
            DataEvent::OrderBook(t) => {
                t.bids.iter().all(BookLevel::has_finite_numbers)
                    && t.asks.iter().all(BookLevel::has_finite_numbers)
            }
            DataEvent::ExternalSignal(t) => {
                t.value.is_none_or(finite) && t.score.is_none_or(finite)
            }
            DataEvent::Heartbeat { .. } => true,
        }
    }
}

impl BookLevel {
    fn has_finite_numbers(&self) -> bool {
        finite(self.price) && finite(self.qty)
    }
}

fn finite(value: f64) -> bool {
    value.is_finite()
}

#[derive(Debug, Clone, Copy)]
pub enum BackpressureMode {
    Block,
    DropNewest,
}

/// Wall-clock observation time, NOT a unique ID or a duration clock.
/// Use an independent sequence for ordering and Instant for elapsed time.
pub fn now_ms() -> u64 {
    unix_ms(SystemTime::now())
}

fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Unknown and substantially future-dated source times are not fresh.
pub fn timestamp_is_fresh(ts: u64, now: u64, ttl_ms: u64) -> bool {
    ts != 0 && ts <= now.saturating_add(1_000) && now.saturating_sub(ts) <= ttl_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_event_rejects_non_finite_market_tick() {
        let event = DataEvent::Tick(MarketTick {
            exchange: "test",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bid: f64::NAN,
            ask: 1.0,
            mark: None,
            funding_rate: None,
            ts_ms: 1,
        });

        assert!(!event.has_finite_numbers());
    }

    #[test]
    fn data_event_accepts_finite_order_book() {
        let event = DataEvent::OrderBook(OrderBookTick {
            exchange: "test",
            market: MarketKind::Spot,
            symbol: "BTCUSDT".into(),
            bids: vec![BookLevel {
                price: 1.0,
                qty: 2.0,
            }],
            asks: vec![BookLevel {
                price: 3.0,
                qty: 4.0,
            }],
            last_update_id: None,
            ts_ms: 1,
        });

        assert!(event.has_finite_numbers());
    }

    #[test]
    fn observation_time_does_not_advance_with_call_count() {
        let fixed = UNIX_EPOCH + std::time::Duration::from_millis(123_456);
        for _ in 0..100_000 {
            assert_eq!(unix_ms(fixed), 123_456);
        }
    }

    #[test]
    fn freshness_rejects_unknown_future_and_expired_timestamps() {
        assert!(!timestamp_is_fresh(0, 10_000, 1_000));
        assert!(!timestamp_is_fresh(11_001, 10_000, 1_000));
        assert!(!timestamp_is_fresh(8_999, 10_000, 1_000));
        assert!(timestamp_is_fresh(9_000, 10_000, 1_000));
        assert!(timestamp_is_fresh(10_001, 10_000, 1_000));
    }
}
