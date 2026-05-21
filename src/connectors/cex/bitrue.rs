use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::parse_value_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const BITRUE_WS_URL: &str = "wss://ws.bitrue.com/market/ws";
const BITRUE_REST_URL: &str = "https://www.bitrue.com/api/v1";

pub struct BitrueSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitrueSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitrueSpotFeed {
    fn name(&self) -> &'static str {
        "bitrue"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitrue spot symbols empty");
        }

        let (ws, _) = connect_async(BITRUE_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        let mut last_trade_ids = HashMap::<String, u64>::new();
        for symbol in &self.symbols {
            let lower = symbol.to_ascii_lowercase();
            sink.send(Message::Text(
                json!({
                    "event": "sub",
                    "params": {
                        "cb_id": lower,
                        "channel": format!("market_{lower}_simple_depth_step0")
                    }
                })
                .to_string(),
            ))
            .await?;
        }

        let mut heartbeat = interval(Duration::from_secs(25));
        let mut trade_poll = interval(Duration::from_secs(5));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = trade_poll.tick() => {
                    for symbol in &self.symbols {
                        let last_seen_trade = last_trade_ids.get(symbol).copied();
                        match poll_bitrue_trades(&self.client, symbol, last_seen_trade).await {
                            Ok((events, max_trade_id)) => {
                                if let Some(trade_id) = max_trade_id {
                                    last_trade_ids.insert(symbol.clone(), trade_id);
                                }
                                for event in events {
                                    ctx.emit(event).await?;
                                }
                            }
                            Err(err) => tracing::warn!(exchange = "bitrue", symbol, error = %err, "trade poll failed"),
                        }
                    }
                }
                _ = heartbeat.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("bitrue heartbeat timeout");
                    }
                    ctx.emit(DataEvent::Heartbeat { exchange: "bitrue", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("bitrue stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            if let Some(pong) = bitrue_pong(&text) {
                                sink.send(Message::Text(pong)).await?;
                                continue;
                            }
                            for event in parse_bitrue_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("bitrue closed"),
                    }
                }
            }
        }
    }
}

async fn poll_bitrue_trades(
    client: &reqwest::Client,
    symbol: &str,
    last_seen_trade: Option<u64>,
) -> Result<(Vec<DataEvent>, Option<u64>)> {
    let value = client
        .get(format!("{BITRUE_REST_URL}/trades"))
        .query(&[("symbol", symbol), ("limit", "100")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_bitrue_trades(symbol, &value, last_seen_trade))
}

fn parse_bitrue_events(text: &str) -> Result<Vec<DataEvent>> {
    let value = serde_json::from_str::<Value>(text)?;
    if !value
        .get("channel")
        .and_then(Value::as_str)
        .is_some_and(|channel| {
            channel.starts_with("market_") && channel.ends_with("_simple_depth_step0")
        })
    {
        return Ok(Vec::new());
    }
    Ok(parse_bitrue_book(&value))
}

fn parse_bitrue_book(value: &Value) -> Vec<DataEvent> {
    let channel = value.get("channel").and_then(Value::as_str).unwrap_or("");
    let symbol = channel
        .strip_prefix("market_")
        .and_then(|x| x.strip_suffix("_simple_depth_step0"))
        .unwrap_or("unknown")
        .to_ascii_uppercase();
    let tick = value.get("tick").unwrap_or(value);
    let ts_ms = value
        .get("ts")
        .or_else(|| tick.get("ts"))
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = tick
        .get("buys")
        .or_else(|| tick.get("bids"))
        .and_then(Value::as_array)
        .map(|items| bitrue_levels(items))
        .unwrap_or_default();
    let asks = tick
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| bitrue_levels(items))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitrue",
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
        exchange: "bitrue",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: Some(ts_ms),
        ts_ms,
    }));
    events
}

fn parse_bitrue_trades(
    symbol: &str,
    value: &Value,
    last_seen_trade: Option<u64>,
) -> (Vec<DataEvent>, Option<u64>) {
    let rows = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut max_trade_id = last_seen_trade;
    let mut events = Vec::new();
    for item in rows {
        let trade_id = item.get("id").and_then(Value::as_u64);
        if trade_id.is_some_and(|id| last_seen_trade.is_some_and(|last| id <= last)) {
            continue;
        }
        if let Some(id) = trade_id {
            max_trade_id = Some(max_trade_id.map_or(id, |max| max.max(id)));
        }
        let Some(price) = item.get("price").and_then(parse_value_f64) else {
            continue;
        };
        let Some(qty) = item.get("qty").and_then(parse_value_f64) else {
            continue;
        };
        events.push(DataEvent::Trade(TradeTick {
            exchange: "bitrue",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            price,
            qty,
            side: match (
                item.get("isBuyer").and_then(Value::as_bool),
                item.get("isBuyerMaker").and_then(Value::as_bool),
            ) {
                (Some(true), _) => TradeSide::Buy,
                (Some(false), _) => TradeSide::Sell,
                (None, Some(true)) => TradeSide::Sell,
                (None, Some(false)) => TradeSide::Buy,
                (None, None) => TradeSide::Unknown,
            },
            trade_id: trade_id.map(|id| id.to_string().into_boxed_str()),
            ts_ms: item
                .get("time")
                .or_else(|| item.get("ctime"))
                .and_then(Value::as_u64)
                .unwrap_or_else(now_ms),
        }));
    }
    (events, max_trade_id)
}

fn bitrue_levels(items: &[Value]) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            let row = item.as_array()?;
            Some(BookLevel {
                price: row.first().and_then(parse_value_f64)?,
                qty: row.get(1).and_then(parse_value_f64)?,
            })
        })
        .collect()
}

fn bitrue_pong(text: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    Some(json!({"pong": value.get("ping")?}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bitrue_parses_book_as_quote_and_book() {
        let events = parse_bitrue_events(
            &json!({
                "channel": "market_btcusdt_simple_depth_step0",
                "ts": 1779290460000_u64,
                "tick": {
                    "buys": [["100.1", "2"]],
                    "asks": [["100.2", "3"]]
                }
            })
            .to_string(),
        )
        .expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        match &events[1] {
            DataEvent::OrderBook(book) => assert_eq!(book.symbol.as_ref(), "BTCUSDT"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bitrue_builds_pong() {
        assert_eq!(
            bitrue_pong(&json!({"ping": 123}).to_string()).as_deref(),
            Some(r#"{"pong":123}"#)
        );
    }

    #[test]
    fn bitrue_parses_recent_trades_with_dedupe() {
        let (events, max_trade_id) = parse_bitrue_trades(
            "BTCUSDT",
            &json!([
                {"id": 10, "price": "100.1", "qty": "0.2", "time": 1779290460001_u64, "isBuyerMaker": true},
                {"id": 11, "price": "100.2", "qty": "0.3", "time": 1779290460002_u64, "isBuyer": true}
            ]),
            Some(10),
        );
        assert_eq!(max_trade_id, Some(11));
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.trade_id.as_deref(), Some("11"));
                assert_eq!(trade.side, TradeSide::Buy);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
