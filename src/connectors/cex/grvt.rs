use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

const GRVT_WS_URL: &str = "wss://market-data.grvt.io/ws/full";

#[derive(Debug, Deserialize)]
struct GrvtMsg {
    #[serde(default)]
    stream: Option<String>,
    #[serde(default)]
    feed: Option<Value>,
}

pub struct GrvtPerpFeed {
    symbols: Vec<String>,
}

impl GrvtPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for GrvtPerpFeed {
    fn name(&self) -> &'static str {
        "grvt"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("grvt perp symbols empty");
        }

        let (ws, _) = connect_async(GRVT_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        let mut request_id = 1u64;
        for symbol in &self.symbols {
            for (stream_name, suffix) in [
                ("v1.book.d", "@100"),
                ("v1.trade", "@50"),
                ("v1.ticker.s", "@1000"),
            ] {
                sink.send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "method": "subscribe",
                        "params": {
                            "stream": stream_name,
                            "selectors": [format!("{symbol}{suffix}")]
                        },
                        "id": request_id
                    })
                    .to_string(),
                ))
                .await?;
                request_id += 1;
            }
        }

        let mut ping_tick = interval(Duration::from_secs(15));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(60) {
                        anyhow::bail!("grvt heartbeat timeout");
                    }
                    sink.send(Message::Ping(Vec::new())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "grvt", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("grvt stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_grvt_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("grvt closed"),
                    }
                }
            }
        }
    }
}

fn parse_grvt_events(text: &str) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") {
        anyhow::bail!("grvt error message: {text}");
    }
    let msg = serde_json::from_str::<GrvtMsg>(text)?;
    let Some(stream) = msg.stream else {
        return Ok(Vec::new());
    };
    let Some(feed) = msg.feed else {
        return Ok(Vec::new());
    };

    match stream.as_str() {
        "v1.book.d" | "v1.book.s" => Ok(parse_grvt_book(&feed)),
        "v1.trade" => Ok(parse_grvt_trade(&feed).into_iter().collect()),
        "v1.ticker.s" => Ok(parse_grvt_ticker(&feed)),
        _ => Ok(Vec::new()),
    }
}

fn parse_grvt_book(feed: &Value) -> Vec<DataEvent> {
    let symbol = feed
        .get("instrument")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    let ts_ms = feed
        .get("event_time")
        .and_then(parse_grvt_timestamp_ms)
        .unwrap_or_else(now_ms);
    let bids = feed
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "size"))
        .unwrap_or_default();
    let asks = feed
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "size"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "grvt",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "grvt",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: feed.get("event_time").and_then(parse_u64),
        ts_ms,
    }));

    events
}

fn parse_grvt_trade(feed: &Value) -> Option<DataEvent> {
    let symbol = feed.get("instrument").and_then(Value::as_str)?;
    Some(DataEvent::Trade(TradeTick {
        exchange: "grvt",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        price: feed.get("price").and_then(parse_value_f64).unwrap_or(0.0),
        qty: feed.get("size").and_then(parse_value_f64).unwrap_or(0.0),
        side: match feed.get("is_taker_buyer").and_then(Value::as_bool) {
            Some(true) => TradeSide::Buy,
            Some(false) => TradeSide::Sell,
            None => TradeSide::Unknown,
        },
        trade_id: feed
            .get("trade_id")
            .and_then(value_to_string)
            .map(String::into_boxed_str),
        ts_ms: feed
            .get("event_time")
            .and_then(parse_grvt_timestamp_ms)
            .unwrap_or_else(now_ms),
    }))
}

fn parse_grvt_ticker(feed: &Value) -> Vec<DataEvent> {
    let symbol = feed
        .get("instrument")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    let ts_ms = feed
        .get("event_time")
        .and_then(parse_grvt_timestamp_ms)
        .unwrap_or_else(now_ms);
    let mark_price = feed.get("mark_price").and_then(parse_value_f64);
    let index_price = feed.get("index_price").and_then(parse_value_f64);
    let funding_rate = feed
        .get("funding_rate_8h_curr")
        .or_else(|| feed.get("funding_rate"))
        .and_then(parse_value_f64);
    let mut events = Vec::with_capacity(3);

    if let (Some(bid), Some(ask)) = (
        feed.get("best_bid_price").and_then(parse_value_f64),
        feed.get("best_ask_price").and_then(parse_value_f64),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "grvt",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: mark_price,
            funding_rate,
            ts_ms,
        }));
    }

    if let Some(funding_rate) = funding_rate {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "grvt",
            symbol: symbol.to_string().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: feed
                .get("next_funding_time")
                .and_then(parse_grvt_timestamp_ms),
            mark_price,
            index_price,
            ts_ms,
        }));
    }

    if let Some(open_interest) = feed.get("open_interest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "grvt",
            symbol: symbol.to_string().into_boxed_str(),
            open_interest,
            open_interest_value: mark_price.map(|mark| mark * open_interest),
            ts_ms,
        }));
    }

    events
}

fn parse_u64(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|x| x.parse::<u64>().ok())
        .or_else(|| value.as_u64())
}

fn parse_grvt_timestamp_ms(value: &Value) -> Option<u64> {
    let raw = parse_u64(value)?;
    Some(if raw > 10_000_000_000_000_000 {
        raw / 1_000_000
    } else if raw > 10_000_000_000_000 {
        raw / 1_000
    } else if raw < 10_000_000_000 {
        raw * 1_000
    } else {
        raw
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_u64().map(|x| x.to_string()))
        .or_else(|| value.as_i64().map(|x| x.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grvt_parses_book_as_quote_and_book() {
        let text = json!({
            "stream": "v1.book.d",
            "feed": {
                "instrument": "BTC_USDT_Perp",
                "event_time": "1779290460034894378",
                "bids": [{"price":"77661.5","size":"2.665"}],
                "asks": [{"price":"77661.6","size":"0.006"}]
            }
        })
        .to_string();

        let events = parse_grvt_events(&text).expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.exchange, "grvt");
                assert_eq!(tick.symbol.as_ref(), "BTC_USDT_Perp");
                assert_eq!(tick.bid, 77661.5);
                assert_eq!(tick.ask, 77661.6);
                assert_eq!(tick.ts_ms, 1_779_290_460_034);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn grvt_parses_trade_side() {
        let text = json!({
            "stream": "v1.trade",
            "feed": {
                "instrument": "BTC_USDT_Perp",
                "event_time": "1779290460034894378",
                "trade_id": "abc",
                "price": "77659.2",
                "size": "4.192",
                "is_taker_buyer": true
            }
        })
        .to_string();

        let events = parse_grvt_events(&text).expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.side, TradeSide::Buy);
                assert_eq!(trade.trade_id.as_deref(), Some("abc"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn grvt_parses_ticker_quote_funding_and_open_interest() {
        let text = json!({
            "stream": "v1.ticker.s",
            "feed": {
                "event_time": "1779290460034894378",
                "instrument": "BTC_USDT_Perp",
                "mark_price": "77673.363700741",
                "index_price": "77697.790709658",
                "best_bid_price": "77661.5",
                "best_ask_price": "77661.6",
                "funding_rate_8h_curr": "0.01",
                "open_interest": "2731.756774874",
                "next_funding_time": "1779292800000000000"
            }
        })
        .to_string();

        let events = parse_grvt_events(&text).expect("events");
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        assert!(matches!(&events[1], DataEvent::FundingRate(_)));
        assert!(matches!(&events[2], DataEvent::OpenInterest(_)));
    }
}
