use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const DERIVE_WS_URL: &str = "wss://api.lyra.finance/ws";

#[derive(Debug, Deserialize)]
struct DeriveMsg {
    #[serde(default)]
    params: Option<DeriveParams>,
}

#[derive(Debug, Deserialize)]
struct DeriveParams {
    channel: String,
    data: Value,
}

pub struct DeriveSpotFeed {
    symbols: Vec<String>,
}

impl DeriveSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for DeriveSpotFeed {
    fn name(&self) -> &'static str {
        "derive"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_derive(MarketKind::Spot, &self.symbols, ctx).await
    }
}

pub struct DerivePerpFeed {
    symbols: Vec<String>,
}

impl DerivePerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for DerivePerpFeed {
    fn name(&self) -> &'static str {
        "derive"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_derive(MarketKind::Perp, &self.symbols, ctx).await
    }
}

async fn run_derive(market: MarketKind, symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        anyhow::bail!("derive symbols empty");
    }

    let (ws, _) = connect_async(DERIVE_WS_URL).await?;
    let (mut sink, mut stream) = ws.split();
    let mut channels = Vec::new();
    for symbol in symbols {
        channels.push(format!("trades.{}", symbol.to_ascii_uppercase()));
        channels.push(match market {
            MarketKind::Spot => format!("orderbook.{}.1.100", symbol.to_ascii_uppercase()),
            MarketKind::Perp => format!("orderbook.{}.10.10", symbol.to_ascii_uppercase()),
        });
        if market == MarketKind::Perp {
            channels.push(format!("ticker_slim.{}.1000", symbol.to_ascii_uppercase()));
        }
    }
    sink.send(Message::Text(
        json!({"method":"subscribe","params":{"channels":channels}}).to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(10));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(45) {
                    anyhow::bail!("derive heartbeat timeout");
                }
                sink.send(Message::Text(json!({"method":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "derive", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("derive stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        for event in parse_derive_events(market, &text)? {
                            ctx.emit(event).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("derive closed"),
                }
            }
        }
    }
}

fn parse_derive_events(market: MarketKind, text: &str) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") {
        anyhow::bail!("derive error message: {text}");
    }
    let msg = serde_json::from_str::<DeriveMsg>(text)?;
    let Some(params) = msg.params else {
        return Ok(Vec::new());
    };

    if params.channel.starts_with("orderbook.") {
        Ok(parse_derive_book(market, &params.data))
    } else if params.channel.starts_with("trades.") {
        Ok(parse_derive_trades(market, &params.data))
    } else if params.channel.starts_with("ticker_slim.") {
        Ok(parse_derive_ticker(&params.channel, &params.data)
            .into_iter()
            .collect())
    } else {
        Ok(Vec::new())
    }
}

fn parse_derive_book(market: MarketKind, data: &Value) -> Vec<DataEvent> {
    let symbol = data
        .get("instrument_name")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    let ts_ms = data
        .get("timestamp")
        .and_then(Value::as_u64)
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
            exchange: "derive",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "derive",
        market,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: data.get("publish_id").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_derive_trades(market: MarketKind, data: &Value) -> Vec<DataEvent> {
    let Some(rows) = data.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let symbol = row.get("instrument_name").and_then(Value::as_str)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "derive",
                market,
                symbol: symbol.to_string().into_boxed_str(),
                price: row
                    .get("trade_price")
                    .and_then(parse_value_f64)
                    .unwrap_or(0.0),
                qty: row
                    .get("trade_amount")
                    .and_then(parse_value_f64)
                    .unwrap_or(0.0),
                side: row
                    .get("direction")
                    .and_then(Value::as_str)
                    .map(side_from_direction)
                    .unwrap_or(TradeSide::Unknown),
                trade_id: row
                    .get("trade_id")
                    .and_then(Value::as_str)
                    .map(|x| x.to_string().into_boxed_str()),
                ts_ms: row
                    .get("timestamp")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_derive_ticker(channel: &str, data: &Value) -> Option<DataEvent> {
    let symbol = channel.split('.').nth(1).unwrap_or("UNKNOWN");
    let ticker = data.get("instrument_ticker")?;
    let funding_rate = ticker.get("f").and_then(parse_value_f64)?;
    Some(DataEvent::FundingRate(FundingRateTick {
        exchange: "derive",
        symbol: symbol.to_string().into_boxed_str(),
        funding_rate,
        next_funding_time_ms: None,
        mark_price: ticker.get("M").and_then(parse_value_f64),
        index_price: ticker.get("I").and_then(parse_value_f64),
        ts_ms: now_ms(),
    }))
}

fn side_from_direction(direction: &str) -> TradeSide {
    side_from_labels(direction, &["buy"], &["sell"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn derive_parses_orderbook_as_quote_and_book() {
        let text = json!({
            "params": {
                "channel": "orderbook.ETH-PERP.10.10",
                "data": {
                    "instrument_name": "ETH-PERP",
                    "publish_id": 7,
                    "timestamp": 1000,
                    "bids": [["100", "2"]],
                    "asks": [["101", "3"]]
                }
            }
        })
        .to_string();

        let events = parse_derive_events(MarketKind::Perp, &text).expect("events");

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn derive_parses_trade_and_funding() {
        let trade_text = json!({
            "params": {
                "channel": "trades.ETH-PERP",
                "data": [{
                    "instrument_name": "ETH-PERP",
                    "direction": "sell",
                    "trade_id": "1",
                    "trade_price": "100",
                    "trade_amount": "2",
                    "timestamp": 1000
                }]
            }
        })
        .to_string();
        let funding_text = json!({
            "params": {
                "channel": "ticker_slim.ETH-PERP.1000",
                "data": {
                    "instrument_ticker": {"I": "99", "M": "100", "f": "0.0001"}
                }
            }
        })
        .to_string();

        let trade_events = parse_derive_events(MarketKind::Perp, &trade_text).expect("trade");
        let funding_events = parse_derive_events(MarketKind::Perp, &funding_text).expect("funding");

        assert!(matches!(trade_events[0], DataEvent::Trade(_)));
        assert!(matches!(funding_events[0], DataEvent::FundingRate(_)));
    }
}
