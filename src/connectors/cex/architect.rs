use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

const ARCHITECT_WS_URL: &str = "wss://gateway.architect.exchange/md/ws";
const ARCHITECT_REST_URL: &str = "https://gateway.architect.exchange";

pub struct ArchitectPerpFeed {
    symbols: Vec<String>,
    bearer_token: Option<String>,
    client: reqwest::Client,
}

impl ArchitectPerpFeed {
    pub fn new(symbols: Vec<String>, bearer_token: Option<String>) -> Self {
        Self {
            symbols,
            bearer_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for ArchitectPerpFeed {
    fn name(&self) -> &'static str {
        "architect"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("architect perp symbols empty");
        }
        let token = self
            .bearer_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .context("architect bearer token missing; set api_key or api_key_env")?;

        let mut request = ARCHITECT_WS_URL.into_client_request()?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {token}")
                .parse()
                .context("invalid architect token header")?,
        );
        let (ws, _) = connect_async(request).await?;
        let (mut sink, mut stream) = ws.split();
        for (idx, symbol) in self.symbols.iter().enumerate() {
            sink.send(Message::Text(
                json!({
                    "request_id": idx + 1,
                    "type": "subscribe",
                    "symbol": symbol,
                    "level": "LEVEL_2"
                })
                .to_string(),
            ))
            .await?;
        }

        let mut ping_tick = interval(Duration::from_secs(15));
        let mut funding_tick = interval(Duration::from_secs(60));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(60) {
                        anyhow::bail!("architect heartbeat timeout");
                    }
                    sink.send(Message::Ping(Vec::new())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "architect", ts_ms: now_ms() }).await?;
                }
                _ = funding_tick.tick() => {
                    for symbol in &self.symbols {
                        match fetch_architect_funding(&self.client, token, symbol).await {
                            Ok(events) => {
                                for event in events {
                                    ctx.emit(event).await?;
                                }
                            }
                            Err(err) => warn!(exchange = "architect", symbol, error = %err, "funding poll failed"),
                        }
                    }
                }
                msg = stream.next() => {
                    let msg = msg.context("architect stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_architect_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("architect closed"),
                    }
                }
            }
        }
    }
}

async fn fetch_architect_funding(
    client: &reqwest::Client,
    token: &str,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let value = client
        .get(format!("{ARCHITECT_REST_URL}/api/funding-rates"))
        .bearer_auth(token)
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_architect_funding(symbol, &value))
}

fn parse_architect_events(text: &str) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") {
        anyhow::bail!("architect error message: {text}");
    }
    let value = serde_json::from_str::<Value>(text)?;
    match value.get("t").and_then(Value::as_str) {
        Some("2") => Ok(parse_architect_book(&value)),
        Some("t") => Ok(parse_architect_trade(&value).into_iter().collect()),
        _ => Ok(Vec::new()),
    }
}

fn parse_architect_book(value: &Value) -> Vec<DataEvent> {
    let symbol = value.get("s").and_then(Value::as_str).unwrap_or("UNKNOWN");
    let ts_ms = seconds_to_ms(value.get("ts")).unwrap_or_else(now_ms);
    let bids = value
        .get("b")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "p", "q"))
        .unwrap_or_default();
    let asks = value
        .get("a")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "p", "q"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "architect",
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
        exchange: "architect",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("tn").and_then(Value::as_u64),
        ts_ms,
    }));
    events
}

fn parse_architect_trade(value: &Value) -> Option<DataEvent> {
    let symbol = value.get("s").and_then(Value::as_str)?;
    Some(DataEvent::Trade(TradeTick {
        exchange: "architect",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        price: value.get("p").and_then(parse_value_f64).unwrap_or(0.0),
        qty: value.get("q").and_then(parse_value_f64).unwrap_or(0.0),
        side: match value.get("d").and_then(Value::as_str) {
            Some("S") | Some("sell") | Some("SELL") => TradeSide::Sell,
            Some("B") | Some("buy") | Some("BUY") => TradeSide::Buy,
            _ => TradeSide::Unknown,
        },
        trade_id: value
            .get("tn")
            .and_then(Value::as_u64)
            .map(|x| x.to_string().into_boxed_str()),
        ts_ms: seconds_to_ms(value.get("ts")).unwrap_or_else(now_ms),
    }))
}

fn parse_architect_funding(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let rows = value
        .as_array()
        .map(Vec::as_slice)
        .or_else(|| {
            value
                .get("funding_rates")
                .or_else(|| value.get("fundingRates"))
                .or_else(|| value.get("rates"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
        .unwrap_or_else(|| std::slice::from_ref(value));
    let Some(row) = rows.first() else {
        return Vec::new();
    };
    let Some(rate) = row
        .get("funding_rate")
        .or_else(|| row.get("rate"))
        .and_then(parse_value_f64)
    else {
        return Vec::new();
    };
    let mut events = vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "architect",
        symbol: symbol.to_string().into_boxed_str(),
        funding_rate: rate,
        next_funding_time_ms: row.get("timestamp_ns").and_then(ns_to_ms),
        mark_price: row
            .get("settlement_price")
            .or_else(|| row.get("mark_price"))
            .and_then(parse_value_f64),
        index_price: row
            .get("benchmark_price")
            .or_else(|| row.get("index_price"))
            .and_then(parse_value_f64),
        ts_ms: now_ms(),
    })];
    if let Some(open_interest) = first_value(
        row,
        &[
            "open_interest",
            "openInterest",
            "oi",
            "open_interest_contracts",
            "openInterestContracts",
        ],
    )
    .and_then(parse_value_f64)
    {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "architect",
            symbol: symbol.to_string().into_boxed_str(),
            open_interest,
            open_interest_value: first_value(
                row,
                &[
                    "open_interest_value",
                    "openInterestValue",
                    "open_interest_notional",
                    "openInterestNotional",
                ],
            )
            .and_then(parse_value_f64),
            ts_ms: now_ms(),
        }));
    }
    events
}

fn first_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(*key))
}

fn seconds_to_ms(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(parse_value_f64)
        .map(|seconds| (seconds * 1000.0) as u64)
}

fn ns_to_ms(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|x| x.parse::<u64>().ok())
        .or_else(|| value.as_u64())
        .map(|ns| ns / 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn architect_parses_book_as_quote_and_book() {
        let text = json!({
            "t": "2",
            "s": "NVDA-PERP",
            "ts": 1779290460.0,
            "tn": 10,
            "b": [{"p":"100.1","q":"2"}],
            "a": [{"p":"100.2","q":"3"}]
        })
        .to_string();
        let events = parse_architect_events(&text).expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn architect_parses_trade() {
        let text = json!({
            "t": "t",
            "s": "NVDA-PERP",
            "ts": 1779290460.0,
            "tn": 11,
            "d": "S",
            "p": "100.1",
            "q": "2"
        })
        .to_string();
        let events = parse_architect_events(&text).expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Sell),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn architect_parses_funding() {
        let events = parse_architect_funding(
            "NVDA-PERP",
            &json!([{
                "funding_rate":"0.0001",
                "timestamp_ns":"1779292800000000000",
                "settlement_price":"100",
                "benchmark_price":"101",
                "open_interest":"1234",
                "open_interest_value":"123400"
            }]),
        );
        assert!(matches!(&events[0], DataEvent::FundingRate(_)));
        match &events[1] {
            DataEvent::OpenInterest(oi) => {
                assert_eq!(oi.open_interest, 1234.0);
                assert_eq!(oi.open_interest_value, Some(123400.0));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
