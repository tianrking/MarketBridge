use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, LiquidationTick, MarketKind, MarketTick,
    OpenInterestTick, OrderBookTick, TradeTick, now_ms,
};

const BITMEX_REST_URL: &str = "https://www.bitmex.com/api/v1";

pub struct BitmexPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitmexPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitmexPerpFeed {
    fn name(&self) -> &'static str {
        "bitmex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitmex perp symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitmex_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bitmex", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bitmex",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bitmex_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let instrument = client
        .get(format!("{BITMEX_REST_URL}/instrument"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitmex_instrument(symbol, &instrument));

    let book = client
        .get(format!("{BITMEX_REST_URL}/orderBook/L2"))
        .query(&[("symbol", symbol), ("depth", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitmex_book(symbol, &book));

    let trades = client
        .get(format!("{BITMEX_REST_URL}/trade"))
        .query(&[("symbol", symbol), ("reverse", "true"), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitmex_trades(symbol, &trades));

    let liquidations = client
        .get(format!("{BITMEX_REST_URL}/liquidation"))
        .query(&[("symbol", symbol), ("reverse", "true"), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitmex_liquidations(symbol, &liquidations));

    Ok(events)
}

fn parse_bitmex_instrument(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(value);
    let symbol = row
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol);
    let ts_ms = row
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339ish_ms)
        .unwrap_or_else(now_ms);
    let mut events = Vec::new();

    if let (Some(bid), Some(ask)) = (
        row.get("bidPrice").and_then(parse_value_f64),
        row.get("askPrice").and_then(parse_value_f64),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitmex",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: row.get("markPrice").and_then(parse_value_f64),
            funding_rate: row.get("fundingRate").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(rate) = row.get("fundingRate").and_then(parse_value_f64) {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "bitmex",
            symbol: symbol.to_string().into_boxed_str(),
            funding_rate: rate,
            next_funding_time_ms: row
                .get("fundingTimestamp")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339ish_ms),
            mark_price: row.get("markPrice").and_then(parse_value_f64),
            index_price: row.get("indicativeSettlePrice").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(oi) = row.get("openInterest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "bitmex",
            symbol: symbol.to_string().into_boxed_str(),
            open_interest: oi,
            open_interest_value: row.get("openValue").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    events
}

fn parse_bitmex_book(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let levels = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let symbol = levels
        .first()
        .and_then(|row| row.get("symbol"))
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol);
    let mut bids = Vec::new();
    let mut asks = Vec::new();

    for row in levels {
        let Some(price) = row.get("price").and_then(parse_value_f64) else {
            continue;
        };
        let Some(qty) = row.get("size").and_then(parse_value_f64) else {
            continue;
        };
        let level = BookLevel { price, qty };
        if row.get("side").and_then(Value::as_str) == Some("Sell") {
            asks.push(level);
        } else {
            bids.push(level);
        }
    }
    bids.sort_by(|a, b| b.price.total_cmp(&a.price));
    asks.sort_by(|a, b| a.price.total_cmp(&b.price));

    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (bids.first(), asks.first()) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitmex",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid: bid.price,
            ask: ask.price,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitmex",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));
    events
}

fn parse_bitmex_trades(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            let symbol = trade
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitmex",
                market: MarketKind::Perp,
                symbol: symbol.to_string().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("size").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["Buy"], &["Sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("trdMatchID")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339ish_ms)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_bitmex_liquidations(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let symbol = row
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Liquidation(LiquidationTick {
                exchange: "bitmex",
                symbol: symbol.to_string().into_boxed_str(),
                side: row
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["Buy"], &["Sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                price: row.get("price").and_then(parse_value_f64)?,
                qty: row
                    .get("leavesQty")
                    .or_else(|| row.get("orderQty"))
                    .and_then(parse_value_f64)?,
                ts_ms: row
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339ish_ms)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_rfc3339ish_ms(value: &str) -> Option<u64> {
    if let Ok(ts) = value.parse::<u64>() {
        return Some(if ts < 10_000_000_000 { ts * 1000 } else { ts });
    }
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let time = time.trim_end_matches('Z');
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second_part = time_parts.next()?;
    let second = second_part
        .split('.')
        .next()
        .and_then(|x| x.parse::<u32>().ok())?;
    Some(unix_ms_from_ymdhms(year, month, day, hour, minute, second))
}

fn unix_ms_from_ymdhms(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> u64 {
    let days = days_from_civil(year, month, day);
    ((days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64) * 1000) as u64
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bitmex_parses_instrument_market_events() {
        let events = parse_bitmex_instrument(
            "XBTUSD",
            &json!([{
                "symbol": "XBTUSD",
                "timestamp": "2026-05-20T12:00:00.000Z",
                "bidPrice": 100.0,
                "askPrice": 101.0,
                "markPrice": 100.5,
                "fundingRate": "0.0001",
                "fundingTimestamp": "2026-05-20T13:00:00.000Z",
                "openInterest": 12345
            }]),
        );
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::FundingRate(_)));
        assert!(matches!(events[2], DataEvent::OpenInterest(_)));
    }

    #[test]
    fn bitmex_parses_book_as_quote_and_book() {
        let events = parse_bitmex_book(
            "XBTUSD",
            &json!([
                {"symbol": "XBTUSD", "side": "Buy", "size": 10, "price": 99.0},
                {"symbol": "XBTUSD", "side": "Sell", "size": 20, "price": 100.0}
            ]),
        );
        assert_eq!(events.len(), 2);
        match &events[1] {
            DataEvent::OrderBook(book) => {
                assert_eq!(book.bids[0].price, 99.0);
                assert_eq!(book.asks[0].qty, 20.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bitmex_parses_trades_and_liquidations() {
        let trades = parse_bitmex_trades(
            "XBTUSD",
            &json!([{
                "timestamp": "2026-05-20T12:00:02.735Z",
                "symbol": "XBTUSD",
                "side": "Buy",
                "size": 2000,
                "price": 6906.5,
                "trdMatchID": "trade-1"
            }]),
        );
        assert!(matches!(trades[0], DataEvent::Trade(_)));

        let liquidations = parse_bitmex_liquidations(
            "XBTUSD",
            &json!([{
                "timestamp": "2026-05-20T12:00:02.735Z",
                "symbol": "XBTUSD",
                "side": "Sell",
                "leavesQty": 1000,
                "price": 6906.0
            }]),
        );
        assert!(matches!(liquidations[0], DataEvent::Liquidation(_)));
    }
}
