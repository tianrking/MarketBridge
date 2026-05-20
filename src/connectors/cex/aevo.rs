use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const AEVO_REST_URL: &str = "https://api.aevo.xyz";
const AEVO_WS_URL: &str = "wss://ws.aevo.xyz";

#[derive(Debug, Deserialize)]
struct AevoMsg {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

pub struct AevoPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl AevoPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for AevoPerpFeed {
    fn name(&self) -> &'static str {
        "aevo"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("aevo perp symbols empty");
        }

        let (ws, _) = connect_async(AEVO_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        let mut subscriptions = Vec::new();
        for symbol in &self.symbols {
            let symbol = symbol.to_ascii_uppercase();
            subscriptions.push(format!("trades:{symbol}"));
            subscriptions.push(format!("orderbook-100ms:{symbol}"));
            subscriptions.push(format!("ticker-500ms:{symbol}"));
        }
        sink.send(Message::Text(
            json!({"op":"subscribe","data":subscriptions}).to_string(),
        ))
        .await?;

        let mut ping_tick = interval(Duration::from_secs(15));
        let mut funding_tick = interval(Duration::from_secs(30));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(60) {
                        anyhow::bail!("aevo heartbeat timeout");
                    }
                    sink.send(Message::Ping(Vec::new())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "aevo", ts_ms: now_ms() }).await?;
                }
                _ = funding_tick.tick() => {
                    for symbol in &self.symbols {
                        match fetch_aevo_funding_events(&self.client, symbol).await {
                            Ok(events) => {
                                for event in events {
                                    ctx.emit(event).await?;
                                }
                            }
                            Err(err) => warn!(exchange = "aevo", symbol, error = %err, "failed to poll funding"),
                        }
                    }
                }
                msg = stream.next() => {
                    let msg = msg.context("aevo stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_aevo_events(&text)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("aevo closed"),
                    }
                }
            }
        }
    }
}

async fn fetch_aevo_funding_events(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let funding = client
        .get(format!("{AEVO_REST_URL}/funding"))
        .query(&[("instrument_name", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let instrument = client
        .get(format!("{AEVO_REST_URL}/instrument/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    Ok(parse_aevo_funding(symbol, &funding, &instrument))
}

fn parse_aevo_events(text: &str) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") {
        anyhow::bail!("aevo error message: {text}");
    }
    let msg = serde_json::from_str::<AevoMsg>(text)?;
    let Some(channel) = msg.channel else {
        return Ok(Vec::new());
    };
    let Some(data) = msg.data else {
        return Ok(Vec::new());
    };

    if channel.starts_with("orderbook-100ms:") {
        Ok(parse_aevo_book(&data))
    } else if channel.starts_with("trades:") {
        Ok(parse_aevo_trades(&data))
    } else if channel.starts_with("ticker-500ms:") {
        Ok(parse_aevo_ticker(&channel, &data).into_iter().collect())
    } else {
        Ok(Vec::new())
    }
}

fn parse_aevo_book(data: &Value) -> Vec<DataEvent> {
    let symbol = data
        .get("instrument_name")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    let ts_ms = data
        .get("last_updated")
        .and_then(parse_timestamp_ms)
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
            exchange: "aevo",
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
        exchange: "aevo",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: data.get("last_updated").and_then(parse_u64),
        ts_ms,
    }));

    events
}

fn parse_aevo_trades(data: &Value) -> Vec<DataEvent> {
    values_as_rows(data)
        .into_iter()
        .filter_map(|row| {
            let symbol = row.get("instrument_name").and_then(Value::as_str)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "aevo",
                market: MarketKind::Perp,
                symbol: symbol.to_string().into_boxed_str(),
                price: row.get("price").and_then(parse_value_f64).unwrap_or(0.0),
                qty: row.get("amount").and_then(parse_value_f64).unwrap_or(0.0),
                side: row
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(TradeSide::Unknown),
                trade_id: row
                    .get("trade_id")
                    .and_then(value_to_string)
                    .map(String::into_boxed_str),
                ts_ms: row
                    .get("created_timestamp")
                    .and_then(parse_timestamp_ms)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_aevo_ticker(channel: &str, data: &Value) -> Option<DataEvent> {
    let ticker = data
        .get("tickers")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .unwrap_or(data);
    let symbol = ticker
        .get("instrument_name")
        .and_then(Value::as_str)
        .or_else(|| channel.split(':').nth(1))
        .unwrap_or("UNKNOWN");
    let bid = nested_price(ticker.get("best_bid"))
        .or_else(|| ticker.get("bid").and_then(parse_value_f64))?;
    let ask = nested_price(ticker.get("best_ask"))
        .or_else(|| ticker.get("ask").and_then(parse_value_f64))?;

    Some(DataEvent::Tick(MarketTick {
        exchange: "aevo",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bid,
        ask,
        mark: nested_price(ticker.get("mark"))
            .or_else(|| ticker.get("mark_price").and_then(parse_value_f64)),
        funding_rate: ticker.get("funding_rate").and_then(parse_value_f64),
        ts_ms: ticker
            .get("timestamp")
            .or_else(|| data.get("timestamp"))
            .and_then(parse_timestamp_ms)
            .unwrap_or_else(now_ms),
    }))
}

fn parse_aevo_funding(symbol: &str, funding: &Value, instrument: &Value) -> Vec<DataEvent> {
    let funding_rate = funding
        .get("funding_rate")
        .or_else(|| instrument.get("funding_rate"))
        .and_then(parse_value_f64);
    let mark_price = instrument.get("mark_price").and_then(parse_value_f64);
    let index_price = instrument.get("index_price").and_then(parse_value_f64);
    let next_funding_time_ms = funding.get("next_epoch").and_then(parse_timestamp_ms);
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let Some(funding_rate) = funding_rate {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "aevo",
            symbol: symbol.to_string().into_boxed_str(),
            funding_rate,
            next_funding_time_ms,
            mark_price,
            index_price,
            ts_ms,
        }));
    }

    if let (Some(bid), Some(ask)) = (
        nested_price(instrument.get("best_bid")),
        nested_price(instrument.get("best_ask")),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "aevo",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: mark_price,
            funding_rate,
            ts_ms,
        }));
    }

    events
}

fn values_as_rows(data: &Value) -> Vec<&Value> {
    data.as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_else(|| vec![data])
}

fn nested_price(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Object(map) => map.get("price").and_then(parse_value_f64),
        other => parse_value_f64(other),
    }
}

fn parse_u64(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|x| x.parse::<u64>().ok())
        .or_else(|| value.as_u64())
}

fn parse_timestamp_ms(value: &Value) -> Option<u64> {
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
    fn aevo_parses_orderbook_as_quote_and_book() {
        let text = json!({
            "channel": "orderbook-100ms:ETH-PERP",
            "data": {
                "type": "snapshot",
                "instrument_name": "ETH-PERP",
                "bids": [["2129.31", "0.87"]],
                "asks": [["2129.62", "0.9"]],
                "last_updated": "1779289910226756180"
            }
        })
        .to_string();

        let events = parse_aevo_events(&text).expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.exchange, "aevo");
                assert_eq!(tick.symbol.as_ref(), "ETH-PERP");
                assert_eq!(tick.bid, 2129.31);
                assert_eq!(tick.ask, 2129.62);
                assert_eq!(tick.ts_ms, 1_779_289_910_226);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn aevo_parses_trade() {
        let text = json!({
            "channel": "trades:ETH-PERP",
            "data": {
                "instrument_name": "ETH-PERP",
                "created_timestamp": "1779289911226756180",
                "side": "buy",
                "trade_id": 123,
                "price": "2129.5",
                "amount": "0.2"
            }
        })
        .to_string();

        let events = parse_aevo_events(&text).expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.symbol.as_ref(), "ETH-PERP");
                assert_eq!(trade.side, TradeSide::Buy);
                assert_eq!(trade.trade_id.as_deref(), Some("123"));
                assert_eq!(trade.ts_ms, 1_779_289_911_226);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn aevo_parses_funding_and_instrument_quote() {
        let events = parse_aevo_funding(
            "ETH-PERP",
            &json!({"next_epoch":"1779292800000000000","funding_rate":"0.000008"}),
            &json!({
                "mark_price":"2129.662945",
                "index_price":"2129.754844",
                "best_bid":{"price":"2129.31","amount":"0.87"},
                "best_ask":{"price":"2129.62","amount":"0.9"}
            }),
        );

        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::FundingRate(funding) => {
                assert_eq!(funding.symbol.as_ref(), "ETH-PERP");
                assert_eq!(funding.funding_rate, 0.000008);
                assert_eq!(funding.next_funding_time_ms, Some(1_779_292_800_000));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(&events[1], DataEvent::Tick(_)));
    }
}
