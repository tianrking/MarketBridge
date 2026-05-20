use async_trait::async_trait;
use std::collections::HashSet;
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

const PACIFICA_WS_URL: &str = "wss://ws.pacifica.fi/ws";

#[derive(Debug, Deserialize)]
struct PacificaMsg {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

pub struct PacificaPerpFeed {
    symbols: Vec<String>,
}

impl PacificaPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for PacificaPerpFeed {
    fn name(&self) -> &'static str {
        "pacifica"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("pacifica perp symbols empty");
        }

        let (ws, _) = connect_async(PACIFICA_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        for symbol in &self.symbols {
            sink.send(Message::Text(
                json!({"method":"subscribe","params":{"source":"book","symbol":symbol,"agg_level":1}})
                    .to_string(),
            ))
            .await?;
            sink.send(Message::Text(
                json!({"method":"subscribe","params":{"source":"trades","symbol":symbol}})
                    .to_string(),
            ))
            .await?;
        }
        sink.send(Message::Text(
            json!({"method":"subscribe","params":{"source":"prices"}}).to_string(),
        ))
        .await?;

        let symbol_filter = self
            .symbols
            .iter()
            .map(|symbol| symbol.to_ascii_uppercase())
            .collect::<HashSet<_>>();
        let mut ping_tick = interval(Duration::from_secs(30));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("pacifica heartbeat timeout");
                    }
                    sink.send(Message::Text(json!({"method":"ping"}).to_string())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "pacifica", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("pacifica stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_pacifica_events(&text, &symbol_filter)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("pacifica closed"),
                    }
                }
            }
        }
    }
}

fn parse_pacifica_events(text: &str, symbol_filter: &HashSet<String>) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") && !text.contains("\"error\":null") {
        anyhow::bail!("pacifica error message: {text}");
    }
    let msg = serde_json::from_str::<PacificaMsg>(text)?;
    let Some(channel) = msg.channel else {
        return Ok(Vec::new());
    };
    let Some(data) = msg.data else {
        return Ok(Vec::new());
    };

    match channel.as_str() {
        "book" => Ok(parse_pacifica_book(&data)),
        "trades" => Ok(parse_pacifica_trades(&data)),
        "prices" => Ok(parse_pacifica_prices(&data, symbol_filter)),
        _ => Ok(Vec::new()),
    }
}

fn parse_pacifica_book(data: &Value) -> Vec<DataEvent> {
    let symbol = data.get("s").and_then(Value::as_str).unwrap_or("UNKNOWN");
    let ts_ms = data.get("t").and_then(Value::as_u64).unwrap_or_else(now_ms);
    let levels = data.get("l").and_then(Value::as_array);
    let bids = levels
        .and_then(|items| items.first())
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "p", "a"))
        .unwrap_or_default();
    let asks = levels
        .and_then(|items| items.get(1))
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "p", "a"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "pacifica",
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
        exchange: "pacifica",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: data.get("li").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_pacifica_trades(data: &Value) -> Vec<DataEvent> {
    let Some(rows) = data.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let symbol = row.get("s").and_then(Value::as_str)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "pacifica",
                market: MarketKind::Perp,
                symbol: symbol.to_string().into_boxed_str(),
                price: row.get("p").and_then(parse_value_f64).unwrap_or(0.0),
                qty: row.get("a").and_then(parse_value_f64).unwrap_or(0.0),
                side: row
                    .get("d")
                    .and_then(Value::as_str)
                    .map(pacifica_trade_side)
                    .unwrap_or(TradeSide::Unknown),
                trade_id: row
                    .get("h")
                    .and_then(value_to_string)
                    .map(String::into_boxed_str),
                ts_ms: row.get("t").and_then(Value::as_u64).unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_pacifica_prices(data: &Value, symbol_filter: &HashSet<String>) -> Vec<DataEvent> {
    let Some(rows) = data.as_array() else {
        return Vec::new();
    };
    let next_funding_time_ms = next_hour_ms();
    let mut events = Vec::new();

    for row in rows {
        let Some(symbol) = row.get("symbol").and_then(Value::as_str) else {
            continue;
        };
        if !symbol_filter.contains(&symbol.to_ascii_uppercase()) {
            continue;
        }
        let ts_ms = row
            .get("timestamp")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms);

        if let Some(funding_rate) = row.get("funding").and_then(parse_value_f64) {
            events.push(DataEvent::FundingRate(FundingRateTick {
                exchange: "pacifica",
                symbol: symbol.to_string().into_boxed_str(),
                funding_rate,
                next_funding_time_ms: Some(next_funding_time_ms),
                mark_price: row.get("mark").and_then(parse_value_f64),
                index_price: row.get("oracle").and_then(parse_value_f64),
                ts_ms,
            }));
        }

        if let Some(open_interest) = row.get("open_interest").and_then(parse_value_f64) {
            events.push(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "pacifica",
                symbol: symbol.to_string().into_boxed_str(),
                open_interest,
                open_interest_value: row
                    .get("mark")
                    .and_then(parse_value_f64)
                    .map(|mark| mark * open_interest),
                ts_ms,
            }));
        }
    }

    events
}

fn pacifica_trade_side(direction: &str) -> TradeSide {
    match direction {
        "open_long" | "close_short" => TradeSide::Buy,
        "open_short" | "close_long" => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

fn next_hour_ms() -> u64 {
    let now_secs = now_ms() / 1000;
    ((now_secs / 3600) + 1) * 3600 * 1000
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

    fn filter() -> HashSet<String> {
        ["BTC".to_string()].into_iter().collect()
    }

    #[test]
    fn pacifica_parses_book_as_quote_and_book() {
        let text = json!({
            "channel": "book",
            "data": {
                "s": "BTC",
                "l": [
                    [{"p":"77677","a":"0.37888","n":3}],
                    [{"p":"77678","a":"0.80598","n":7}]
                ],
                "t": 1779290245504u64,
                "li": 1559885104u64
            }
        })
        .to_string();

        let events = parse_pacifica_events(&text, &filter()).expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.exchange, "pacifica");
                assert_eq!(tick.symbol.as_ref(), "BTC");
                assert_eq!(tick.bid, 77677.0);
                assert_eq!(tick.ask, 77678.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn pacifica_parses_trade_direction() {
        let text = json!({
            "channel": "trades",
            "data": [{
                "h": 80062522u64,
                "s": "BTC",
                "a": "0.00001",
                "p": "89471",
                "d": "close_short",
                "t": 1765018379085u64
            }]
        })
        .to_string();

        let events = parse_pacifica_events(&text, &filter()).expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.symbol.as_ref(), "BTC");
                assert_eq!(trade.side, TradeSide::Buy);
                assert_eq!(trade.trade_id.as_deref(), Some("80062522"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn pacifica_parses_prices_as_funding_and_open_interest() {
        let text = json!({
            "channel": "prices",
            "data": [{
                "funding": "0.000015",
                "mark": "77686.16918",
                "open_interest": "479.84721",
                "oracle": "77712.197261",
                "symbol": "BTC",
                "timestamp": 1779290244392u64
            }, {
                "funding": "0.00000854",
                "mark": "2144.772564",
                "open_interest": "12712.9228",
                "oracle": "2145.725581",
                "symbol": "ETH",
                "timestamp": 1779290244392u64
            }]
        })
        .to_string();

        let events = parse_pacifica_events(&text, &filter()).expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::FundingRate(_)));
        assert!(matches!(&events[1], DataEvent::OpenInterest(_)));
    }
}
