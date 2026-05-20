use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy)]
pub enum BackpressureMode {
    Block,
    DropNewest,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
