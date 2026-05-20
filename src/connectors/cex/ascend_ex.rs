use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const ASCENDEX_WS_URL: &str =
    "wss://ascendex.com:443/api/pro/v1/websocket-for-hummingbot-liq-mining/stream";

pub struct AscendExSpotFeed {
    symbols: Vec<String>,
}

impl AscendExSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for AscendExSpotFeed {
    fn name(&self) -> &'static str {
        "ascend_ex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("ascend_ex spot symbols empty");
        }

        let (ws, _) = connect_async(ASCENDEX_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        for symbol in &self.symbols {
            for topic in ["depth", "trades"] {
                sink.send(Message::Text(
                    json!({"op": "sub", "ch": format!("{topic}:{symbol}")}).to_string(),
                ))
                .await?;
            }
        }

        let mut heartbeat = interval(Duration::from_secs(25));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("ascend_ex heartbeat timeout");
                    }
                    ctx.emit(DataEvent::Heartbeat { exchange: "ascend_ex", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("ascend_ex stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            if is_ascendex_ping(&text) {
                                sink.send(Message::Text(json!({"op": "pong"}).to_string())).await?;
                                continue;
                            }
                            for event in parse_ascendex_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("ascend_ex closed"),
                    }
                }
            }
        }
    }
}

fn parse_ascendex_events(text: &str) -> Result<Vec<DataEvent>> {
    let value = serde_json::from_str::<Value>(text)?;
    match value.get("m").and_then(Value::as_str) {
        Some("depth") => Ok(parse_ascendex_depth(&value)),
        Some("trades") => Ok(parse_ascendex_trades(&value)),
        _ => Ok(Vec::new()),
    }
}

fn parse_ascendex_depth(value: &Value) -> Vec<DataEvent> {
    let symbol = ascendex_symbol(value);
    let data = value.get("data").unwrap_or(value);
    let ts_ms = data
        .get("ts")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = data
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = data
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "ascend_ex",
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
        exchange: "ascend_ex",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: Some(ts_ms),
        ts_ms,
    }));
    events
}

fn parse_ascendex_trades(value: &Value) -> Vec<DataEvent> {
    let symbol = ascendex_symbol(value);
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "ascend_ex",
                market: MarketKind::Spot,
                symbol: symbol.clone().into_boxed_str(),
                price: trade.get("p").and_then(parse_value_f64)?,
                qty: trade.get("q").and_then(parse_value_f64)?,
                side: match trade.get("bm").and_then(Value::as_bool) {
                    Some(true) => TradeSide::Buy,
                    Some(false) => TradeSide::Sell,
                    None => TradeSide::Unknown,
                },
                trade_id: trade
                    .get("seqnum")
                    .or_else(|| trade.get("ts"))
                    .map(value_to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("ts")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn is_ascendex_ping(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("m").and_then(Value::as_str).map(str::to_string))
        .is_some_and(|kind| kind == "ping")
}

fn ascendex_symbol(value: &Value) -> String {
    value
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN")
        .replace('/', "")
        .to_ascii_uppercase()
}

fn value_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ascendex_parses_depth_as_quote_and_book() {
        let events = parse_ascendex_events(
            &json!({
                "m": "depth",
                "symbol": "BTC/USDT",
                "data": {
                    "ts": 1779290460000_u64,
                    "bids": [["100.1", "2"]],
                    "asks": [["100.2", "3"]]
                }
            })
            .to_string(),
        )
        .expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn ascendex_parses_trades_and_ping() {
        assert!(is_ascendex_ping(&json!({"m": "ping"}).to_string()));
        let events = parse_ascendex_events(
            &json!({
                "m": "trades",
                "symbol": "BTC/USDT",
                "data": [{"p": "100", "q": "0.5", "ts": 1779290460000_u64, "bm": true}]
            })
            .to_string(),
        )
        .expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Buy),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
