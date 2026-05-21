use async_trait::async_trait;
use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use flate2::read::DeflateDecoder;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{
    first_str, parse_array_levels, parse_object_levels, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

const SPOT_WS_URL: &str = "wss://ws-manager-compress.bitmart.com/api?protocol=1.1";
const PERP_WS_URL: &str = "wss://openapi-ws-v2.bitmart.com/api?protocol=1.1";
const BITMART_REST_URL: &str = "https://api-cloud-v2.bitmart.com";

#[derive(Debug, Deserialize)]
struct BitmartWsMsg {
    #[serde(default)]
    table: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

pub struct BitmartSpotFeed {
    pub symbols: Vec<String>,
}

impl BitmartSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BitmartSpotFeed {
    fn name(&self) -> &'static str {
        "bitmart"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bitmart_spot(&self.symbols, ctx).await
    }
}

pub struct BitmartPerpFeed {
    pub symbols: Vec<String>,
}

impl BitmartPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BitmartPerpMetricsPoller {
    pub symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitmartPerpMetricsPoller {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitmartPerpMetricsPoller {
    fn name(&self) -> &'static str {
        "bitmart"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitmart metrics symbols empty");
        }

        let mut tick = interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitmart_metrics(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "bitmart", symbol, error = %err, "metrics poll failed")
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ExchangeSource for BitmartPerpFeed {
    fn name(&self) -> &'static str {
        "bitmart"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bitmart_perp(&self.symbols, ctx).await
    }
}

async fn poll_bitmart_metrics(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::with_capacity(2);

    let funding = client
        .get(format!("{BITMART_REST_URL}/contract/public/funding-rate"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(event) = parse_bitmart_funding(bitmart_payload(&funding)) {
        events.push(event);
    }

    let interest = client
        .get(format!("{BITMART_REST_URL}/contract/public/open-interest"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(event) = parse_bitmart_open_interest(bitmart_payload(&interest)) {
        events.push(event);
    }

    Ok(events)
}

async fn run_bitmart_spot(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        anyhow::bail!("bitmart spot symbols empty");
    }

    let (ws, _) = connect_async(SPOT_WS_URL).await?;
    let (mut sink, mut stream) = ws.split();
    let args = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("spot/trade:{symbol}"),
                format!("spot/depth50:{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"op":"subscribe","args":args}).to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(15));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(60) {
                    anyhow::bail!("bitmart spot heartbeat timeout");
                }
                sink.send(Message::Text(json!({"op":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "bitmart", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("bitmart spot stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        emit_bitmart_text(MarketKind::Spot, &text, &ctx).await?;
                    }
                    Message::Binary(bytes) => {
                        last_seen = Instant::now();
                        let text = decode_bitmart_ws(&bytes)?;
                        emit_bitmart_text(MarketKind::Spot, &text, &ctx).await?;
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("bitmart spot closed"),
                }
            }
        }
    }
}

async fn run_bitmart_perp(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        anyhow::bail!("bitmart perp symbols empty");
    }

    let (ws, _) = connect_async(PERP_WS_URL).await?;
    let (mut sink, mut stream) = ws.split();
    let mut args = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("futures/depthIncrease50:{symbol}"),
                format!("futures/trade:{symbol}"),
                format!("futures/fundingRate:{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    args.push("futures/ticker".to_string());
    sink.send(Message::Text(
        json!({"action":"subscribe","args":args}).to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(75) {
                    anyhow::bail!("bitmart perp heartbeat timeout");
                }
                sink.send(Message::Text("ping".into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "bitmart", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("bitmart perp stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        emit_bitmart_text(MarketKind::Perp, &text, &ctx).await?;
                    }
                    Message::Binary(bytes) => {
                        last_seen = Instant::now();
                        let text = decode_bitmart_ws(&bytes)?;
                        emit_bitmart_text(MarketKind::Perp, &text, &ctx).await?;
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("bitmart perp closed"),
                }
            }
        }
    }
}

async fn emit_bitmart_text(market: MarketKind, text: &str, ctx: &SourceContext) -> Result<()> {
    if text == "pong" || text.contains("\"pong\"") || text.contains("\"subscribe\"") {
        return Ok(());
    }
    for event in parse_bitmart_events(market, text)? {
        ctx.emit(event).await?;
    }
    Ok(())
}

fn decode_bitmart_ws(bytes: &[u8]) -> Result<String> {
    let mut decoder = DeflateDecoder::new(bytes);
    let mut text = String::new();
    match decoder.read_to_string(&mut text) {
        Ok(_) => Ok(text),
        Err(_) => String::from_utf8(bytes.to_vec()).context("bitmart binary message was not utf8"),
    }
}

fn parse_bitmart_events(market: MarketKind, text: &str) -> Result<Vec<DataEvent>> {
    let msg = serde_json::from_str::<BitmartWsMsg>(text)?;
    let channel = msg.table.or(msg.group).unwrap_or_default();
    let Some(data) = msg.data else {
        return Ok(Vec::new());
    };
    let rows = match data {
        Value::Array(items) => items,
        item @ Value::Object(_) => vec![item],
        _ => Vec::new(),
    };

    let mut events = Vec::new();
    for row in rows {
        if channel.contains("depth") {
            events.extend(parse_bitmart_book(market, &row));
        } else if channel.contains("trade") {
            if let Some(event) = parse_bitmart_trade(market, &row) {
                events.push(event);
            }
        } else if channel.contains("fundingRate") {
            if let Some(event) = parse_bitmart_funding(&row) {
                events.push(event);
            }
        } else if channel.contains("ticker") {
            events.extend(parse_bitmart_ticker(&row));
        }
    }

    Ok(events)
}

fn parse_bitmart_book(market: MarketKind, row: &Value) -> Vec<DataEvent> {
    let symbol = first_str(row, &["symbol"]).unwrap_or("UNKNOWN");
    let ts_ms = first_str(row, &["ms_t", "timestamp", "ts"])
        .and_then(|x| x.parse::<u64>().ok())
        .or_else(|| row.get("ms_t").and_then(Value::as_u64))
        .or_else(|| row.get("timestamp").and_then(Value::as_u64))
        .map(normalize_ts_ms)
        .unwrap_or_else(now_ms);
    let bids = parse_levels(row.get("bids"));
    let asks = parse_levels(row.get("asks"));
    let mut events = Vec::with_capacity(2);

    if let (Some(best_bid), Some(best_ask)) = (best_bid(&bids), best_ask(&asks)) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitmart",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bid: best_bid,
            ask: best_ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitmart",
        market,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: first_str(row, &["version"])
            .and_then(|x| x.parse::<u64>().ok())
            .or_else(|| row.get("version").and_then(Value::as_u64)),
        ts_ms,
    }));

    events
}

fn parse_bitmart_trade(market: MarketKind, row: &Value) -> Option<DataEvent> {
    let symbol = first_str(row, &["symbol"])?;
    let ts_ms = first_str(row, &["s_t", "ms_t", "created_at"])
        .and_then(parse_time_like)
        .unwrap_or_else(now_ms);
    let price = first_str(row, &["price", "deal_price"])
        .and_then(|x| x.parse::<f64>().ok())
        .or_else(|| row.get("price").and_then(parse_value_f64))
        .or_else(|| row.get("deal_price").and_then(parse_value_f64))
        .unwrap_or(0.0);
    let qty = first_str(row, &["size", "deal_vol", "vol"])
        .and_then(|x| x.parse::<f64>().ok())
        .or_else(|| row.get("size").and_then(parse_value_f64))
        .or_else(|| row.get("deal_vol").and_then(parse_value_f64))
        .or_else(|| row.get("vol").and_then(parse_value_f64))
        .unwrap_or(0.0);
    let side = first_str(row, &["side"])
        .map(side_from_str)
        .or_else(|| row.get("way").and_then(Value::as_i64).map(side_from_way))
        .unwrap_or(TradeSide::Unknown);

    Some(DataEvent::Trade(TradeTick {
        exchange: "bitmart",
        market,
        symbol: symbol.to_string().into_boxed_str(),
        price,
        qty,
        side,
        trade_id: first_str(row, &["trade_id", "s_t"]).map(|x| x.to_string().into_boxed_str()),
        ts_ms,
    }))
}

fn parse_bitmart_funding(row: &Value) -> Option<DataEvent> {
    let symbol = first_str(row, &["symbol"])?;
    let funding_rate = first_str(row, &["fundingRate", "funding_rate", "expected_rate"])
        .and_then(|x| x.parse::<f64>().ok())?;
    Some(DataEvent::FundingRate(FundingRateTick {
        exchange: "bitmart",
        symbol: symbol.to_string().into_boxed_str(),
        funding_rate,
        next_funding_time_ms: first_str(row, &["nextFundingTime", "funding_time"])
            .and_then(|x| x.parse::<u64>().ok())
            .map(normalize_ts_ms),
        mark_price: first_str(row, &["mark_price", "markPrice"]).and_then(|x| x.parse().ok()),
        index_price: first_str(row, &["index_price", "indexPrice"]).and_then(|x| x.parse().ok()),
        ts_ms: now_ms(),
    }))
}

fn parse_bitmart_open_interest(row: &Value) -> Option<DataEvent> {
    let symbol = first_str(row, &["symbol"])?;
    let open_interest = first_str(row, &["open_interest", "openInterest"])
        .and_then(|x| x.parse::<f64>().ok())
        .or_else(|| row.get("open_interest").and_then(parse_value_f64))?;
    Some(DataEvent::OpenInterest(OpenInterestTick {
        exchange: "bitmart",
        symbol: symbol.to_string().into_boxed_str(),
        open_interest,
        open_interest_value: first_str(row, &["open_interest_value", "openInterestValue"])
            .and_then(|x| x.parse::<f64>().ok())
            .or_else(|| row.get("open_interest_value").and_then(parse_value_f64)),
        ts_ms: first_str(row, &["timestamp", "ts"])
            .and_then(|x| x.parse::<u64>().ok())
            .or_else(|| row.get("timestamp").and_then(Value::as_u64))
            .map(normalize_ts_ms)
            .unwrap_or_else(now_ms),
    }))
}

fn bitmart_payload(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn parse_bitmart_ticker(row: &Value) -> Vec<DataEvent> {
    let symbol = first_str(row, &["symbol"]).unwrap_or("UNKNOWN");
    let ts_ms = first_str(row, &["ms_t", "timestamp", "ts"])
        .and_then(|x| x.parse::<u64>().ok())
        .map(normalize_ts_ms)
        .unwrap_or_else(now_ms);
    let bid =
        first_str(row, &["best_bid", "bid_price", "bidPrice"]).and_then(|x| x.parse::<f64>().ok());
    let ask =
        first_str(row, &["best_ask", "ask_price", "askPrice"]).and_then(|x| x.parse::<f64>().ok());
    let mark = first_str(row, &["mark_price", "markPrice", "fair_price"])
        .and_then(|x| x.parse::<f64>().ok());
    let funding_rate =
        first_str(row, &["funding_rate", "fundingRate"]).and_then(|x| x.parse::<f64>().ok());

    let mut events = Vec::new();
    if let (Some(bid), Some(ask)) = (bid, ask) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitmart",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark,
            funding_rate,
            ts_ms,
        }));
    }
    if let Some(funding_rate) = funding_rate {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "bitmart",
            symbol: symbol.to_string().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: first_str(row, &["nextFundingTime", "funding_time"])
                .and_then(|x| x.parse::<u64>().ok())
                .map(normalize_ts_ms),
            mark_price: mark,
            index_price: first_str(row, &["index_price", "indexPrice"])
                .and_then(|x| x.parse::<f64>().ok()),
            ts_ms,
        }));
    }

    events
}

fn parse_levels(value: Option<&Value>) -> Vec<BookLevel> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            let array_levels = parse_array_levels(items);
            if array_levels.is_empty() {
                parse_object_levels(items, "price", "vol")
            } else {
                array_levels
            }
        })
        .unwrap_or_default()
}

fn best_bid(levels: &[BookLevel]) -> Option<f64> {
    levels.iter().map(|level| level.price).reduce(f64::max)
}

fn best_ask(levels: &[BookLevel]) -> Option<f64> {
    levels.iter().map(|level| level.price).reduce(f64::min)
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["buy", "b"], &["sell", "s"])
}

fn side_from_way(way: i64) -> TradeSide {
    match way {
        1 | 2 | 5 | 8 => TradeSide::Buy,
        3 | 4 | 6 | 7 => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

fn parse_time_like(value: &str) -> Option<u64> {
    if let Ok(ts) = value.parse::<u64>() {
        return Some(normalize_ts_ms(ts));
    }
    None
}

fn normalize_ts_ms(ts: u64) -> u64 {
    if ts < 10_000_000_000 { ts * 1000 } else { ts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TradeSide;
    use serde_json::json;

    #[test]
    fn bitmart_parses_spot_depth_as_quote_and_book() {
        let text = json!({
            "table": "spot/depth50",
            "data": [{
                "symbol": "BTC_USDT",
                "ms_t": 1000,
                "bids": [["100.0", "2"]],
                "asks": [["101.0", "3"]]
            }]
        })
        .to_string();

        let events = parse_bitmart_events(MarketKind::Spot, &text).expect("events");

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn bitmart_parses_perp_trade_way() {
        let text = json!({
            "group": "futures/trade:BTCUSDT",
            "data": [{
                "symbol": "BTCUSDT",
                "trade_id": "1",
                "deal_price": "100",
                "deal_vol": "2",
                "way": 3,
                "created_at": "1000"
            }]
        })
        .to_string();

        let events = parse_bitmart_events(MarketKind::Perp, &text).expect("events");

        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Sell),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bitmart_parses_funding_rate() {
        let text = json!({
            "group": "futures/fundingRate:BTCUSDT",
            "data": {
                "symbol": "BTCUSDT",
                "fundingRate": "0.0001",
                "nextFundingTime": "100000"
            }
        })
        .to_string();

        let events = parse_bitmart_events(MarketKind::Perp, &text).expect("events");

        assert!(matches!(events[0], DataEvent::FundingRate(_)));
    }

    #[test]
    fn bitmart_parses_open_interest() {
        let event = parse_bitmart_open_interest(&json!({
            "timestamp": 1694657502415_u64,
            "symbol": "BTCUSDT",
            "open_interest": "265231.721368593081729069",
            "open_interest_value": "7006353.83988919"
        }))
        .expect("open interest event");

        match event {
            DataEvent::OpenInterest(tick) => {
                assert_eq!(tick.symbol.as_ref(), "BTCUSDT");
                assert!((tick.open_interest - 265_231.721_368_593_1).abs() < f64::EPSILON);
                assert_eq!(tick.open_interest_value, Some(7006353.83988919));
                assert_eq!(tick.ts_ms, 1694657502415);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
