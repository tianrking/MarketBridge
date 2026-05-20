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

const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

#[derive(Debug, Deserialize)]
struct BitstampMsg {
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

pub struct BitstampSpotFeed {
    pub symbols: Vec<String>,
}

impl BitstampSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BitstampSpotFeed {
    fn name(&self) -> &'static str {
        "bitstamp"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitstamp spot symbols empty");
        }

        let (ws, _) = connect_async(BITSTAMP_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        for symbol in &self.symbols {
            let symbol = symbol.to_ascii_lowercase();
            for channel in [
                format!("live_trades_{symbol}"),
                format!("diff_order_book_{symbol}"),
            ] {
                sink.send(Message::Text(
                    json!({"event":"bts:subscribe","data":{"channel":channel}}).to_string(),
                ))
                .await?;
            }
        }

        let mut ping_tick = interval(Duration::from_secs(25));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("bitstamp spot heartbeat timeout");
                    }
                    ctx.emit(DataEvent::Heartbeat { exchange: "bitstamp", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("bitstamp spot stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_bitstamp_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("bitstamp spot closed"),
                    }
                }
            }
        }
    }
}

fn parse_bitstamp_events(text: &str) -> Result<Vec<DataEvent>> {
    let msg = serde_json::from_str::<BitstampMsg>(text)?;
    let event = msg.event.unwrap_or_default();
    let channel = msg.channel.unwrap_or_default();
    if event == "bts:subscription_succeeded" || event == "bts:heartbeat" {
        return Ok(Vec::new());
    }
    if event == "bts:request_reconnect" {
        anyhow::bail!("bitstamp requested reconnect");
    }

    let Some(data) = msg.data else {
        return Ok(Vec::new());
    };

    if channel.starts_with("diff_order_book_") {
        Ok(parse_bitstamp_book(&channel, &data))
    } else if channel.starts_with("live_trades_") {
        Ok(parse_bitstamp_trade(&channel, &data).into_iter().collect())
    } else {
        Ok(Vec::new())
    }
}

fn parse_bitstamp_book(channel: &str, data: &Value) -> Vec<DataEvent> {
    let symbol = symbol_from_channel(channel, "diff_order_book_");
    let ts_ms = parse_timestamp_ms(data).unwrap_or_else(now_ms);
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
            exchange: "bitstamp",
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
        exchange: "bitstamp",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: data
            .get("microtimestamp")
            .and_then(Value::as_str)
            .and_then(|x| x.parse::<u64>().ok()),
        ts_ms,
    }));

    events
}

fn parse_bitstamp_trade(channel: &str, data: &Value) -> Option<DataEvent> {
    let symbol = symbol_from_channel(channel, "live_trades_");
    Some(DataEvent::Trade(TradeTick {
        exchange: "bitstamp",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        price: data.get("price").and_then(parse_value_f64).unwrap_or(0.0),
        qty: data.get("amount").and_then(parse_value_f64).unwrap_or(0.0),
        side: match data.get("type").and_then(Value::as_i64) {
            Some(0) => TradeSide::Buy,
            Some(1) => TradeSide::Sell,
            _ => TradeSide::Unknown,
        },
        trade_id: data
            .get("id")
            .and_then(|value| value.as_i64().map(|x| x.to_string()))
            .or_else(|| {
                data.get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .map(String::into_boxed_str),
        ts_ms: parse_timestamp_ms(data).unwrap_or_else(now_ms),
    }))
}

fn parse_timestamp_ms(data: &Value) -> Option<u64> {
    data.get("microtimestamp")
        .and_then(Value::as_str)
        .and_then(|x| x.parse::<u64>().ok())
        .map(|micros| micros / 1000)
        .or_else(|| {
            data.get("timestamp")
                .and_then(Value::as_str)
                .and_then(|x| x.parse::<u64>().ok())
                .map(|secs| secs * 1000)
        })
}

fn symbol_from_channel(channel: &str, prefix: &str) -> String {
    channel
        .strip_prefix(prefix)
        .unwrap_or(channel)
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bitstamp_parses_book_as_quote_and_book() {
        let text = json!({
            "event": "data",
            "channel": "diff_order_book_btcusd",
            "data": {
                "timestamp": "1000",
                "microtimestamp": "1000000000",
                "bids": [["100", "2"]],
                "asks": [["101", "3"]]
            }
        })
        .to_string();

        let events = parse_bitstamp_events(&text).expect("events");

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn bitstamp_parses_trade_side() {
        let text = json!({
            "event": "trade",
            "channel": "live_trades_btcusd",
            "data": {
                "id": 1,
                "microtimestamp": "1000000000",
                "amount": "2",
                "price": "100",
                "type": 1
            }
        })
        .to_string();

        let events = parse_bitstamp_events(&text).expect("events");

        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Sell),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
