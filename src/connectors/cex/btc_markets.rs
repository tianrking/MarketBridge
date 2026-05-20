use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const BTC_MARKETS_WS_URL: &str = "wss://socket.btcmarkets.net/v2";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BtcMarketsMsg {
    message_type: String,
    market_id: Option<String>,
    snapshot_id: Option<u64>,
    timestamp: Option<String>,
    bids: Option<Vec<Value>>,
    asks: Option<Vec<Value>>,
    trade_id: Option<String>,
    side: Option<String>,
    price: Option<Value>,
    volume: Option<Value>,
}

pub struct BtcMarketsSpotFeed {
    pub symbols: Vec<String>,
}

impl BtcMarketsSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BtcMarketsSpotFeed {
    fn name(&self) -> &'static str {
        "btc_markets"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("btc_markets spot symbols empty");
        }

        let (ws, _) = connect_async(BTC_MARKETS_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        sink.send(Message::Text(
            json!({
                "messageType": "subscribe",
                "marketIds": self.symbols,
                "channels": ["orderbookUpdate", "orderbook", "trade", "heartbeat"]
            })
            .to_string(),
        ))
        .await?;

        let mut ping_tick = interval(Duration::from_secs(30));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("btc_markets spot heartbeat timeout");
                    }
                    ctx.emit(DataEvent::Heartbeat { exchange: "btc_markets", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("btc_markets spot stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_btc_markets_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("btc_markets spot closed"),
                    }
                }
            }
        }
    }
}

fn parse_btc_markets_events(text: &str) -> Result<Vec<DataEvent>> {
    let msg = serde_json::from_str::<BtcMarketsMsg>(text)?;
    match msg.message_type.as_str() {
        "orderbook" | "orderbookUpdate" => Ok(parse_book(msg)),
        "trade" => Ok(parse_trade(msg).into_iter().collect()),
        "heartbeat" | "subscribe" => Ok(Vec::new()),
        "error" => anyhow::bail!("btc_markets error message: {text}"),
        _ => Ok(Vec::new()),
    }
}

fn parse_book(msg: BtcMarketsMsg) -> Vec<DataEvent> {
    let symbol = msg.market_id.unwrap_or_else(|| "UNKNOWN".to_string());
    let ts_ms = msg
        .timestamp
        .as_deref()
        .and_then(parse_rfc3339ish_ms)
        .unwrap_or_else(now_ms);
    let bids = msg
        .bids
        .as_deref()
        .map(parse_array_levels)
        .unwrap_or_default();
    let asks = msg
        .asks
        .as_deref()
        .map(parse_array_levels)
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "btc_markets",
            market: MarketKind::Spot,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "btc_markets",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: msg.snapshot_id,
        ts_ms,
    }));

    events
}

fn parse_trade(msg: BtcMarketsMsg) -> Option<DataEvent> {
    let symbol = msg.market_id?;
    Some(DataEvent::Trade(TradeTick {
        exchange: "btc_markets",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        price: msg.price.as_ref().and_then(parse_value_f64).unwrap_or(0.0),
        qty: msg.volume.as_ref().and_then(parse_value_f64).unwrap_or(0.0),
        side: match msg.side.as_deref() {
            Some("Bid") => TradeSide::Buy,
            Some("Ask") => TradeSide::Sell,
            _ => TradeSide::Unknown,
        },
        trade_id: msg.trade_id.map(String::into_boxed_str),
        ts_ms: msg
            .timestamp
            .as_deref()
            .and_then(parse_rfc3339ish_ms)
            .unwrap_or_else(now_ms),
    }))
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
    fn btc_markets_parses_orderbook_as_quote_and_book() {
        let text = json!({
            "messageType": "orderbook",
            "marketId": "BTC-AUD",
            "snapshotId": 7,
            "timestamp": "2026-05-20T00:00:00.000Z",
            "bids": [["100", "2"]],
            "asks": [["101", "3"]]
        })
        .to_string();

        let events = parse_btc_markets_events(&text).expect("events");

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn btc_markets_parses_trade_side() {
        let text = json!({
            "messageType": "trade",
            "marketId": "BTC-AUD",
            "tradeId": "1",
            "timestamp": "2026-05-20T00:00:00.000Z",
            "side": "Ask",
            "price": "100",
            "volume": "2"
        })
        .to_string();

        let events = parse_btc_markets_events(&text).expect("events");

        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Sell),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
