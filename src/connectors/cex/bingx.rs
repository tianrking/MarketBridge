use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::io::Read;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{
    emit_tick_ext, first_str, parse_array_levels, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick, TradeSide, TradeTick,
    now_ms,
};

const BINGX_SWAP_REST_URL: &str = "https://open-api.bingx.com/openApi/swap/v2";

pub struct BingxSwapFeed {
    symbols: Vec<String>,
}

impl BingxSwapFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BingxSwapMetricsPoller {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BingxSwapMetricsPoller {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BingxSwapMetricsPoller {
    fn name(&self) -> &'static str {
        "bingx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            bail!("bingx metrics symbols empty");
        }

        let mut tick = interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bingx_metrics(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "bingx", symbol, error = %err, "metrics poll failed")
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ExchangeSource for BingxSwapFeed {
    fn name(&self) -> &'static str {
        "bingx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bingx_swap(&self.symbols, ctx).await
    }
}

async fn poll_bingx_metrics(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::with_capacity(2);

    let premium = client
        .get(format!("{BINGX_SWAP_REST_URL}/quote/premiumIndex"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(event) = parse_bingx_funding(symbol, &premium) {
        events.push(event);
    }

    let interest = client
        .get(format!("{BINGX_SWAP_REST_URL}/quote/openInterest"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(event) = parse_bingx_open_interest(symbol, &interest) {
        events.push(event);
    }

    Ok(events)
}

async fn run_bingx_swap(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("bingx symbols empty");
    }
    let (ws, _) = connect_async("wss://open-api-swap.bingx.com/swap-api")
        .await
        .context("bingx connect failed")?;
    let (mut sink, mut stream) = ws.split();
    for symbol in symbols {
        for data_type in ["ticker", "depth20", "trade"] {
            sink.send(Message::Text(
                json!({"id":format!("{symbol}-{data_type}"),"reqType":"sub","dataType":format!("{symbol}@{data_type}")})
                    .to_string(),
            ))
            .await?;
        }
    }
    let mut ping = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    bail!("bingx pong timeout");
                }
                sink.send(Message::Text("Ping".into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "bingx", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("bingx stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        handle_text(&text, &ctx).await?;
                    }
                    Message::Binary(bytes) => {
                        last_seen = Instant::now();
                        if let Some(text) = decode_gzip_text(&bytes) {
                            handle_text(&text, &ctx).await?;
                        }
                    }
                    Message::Ping(payload) => {
                        last_seen = Instant::now();
                        sink.send(Message::Pong(payload)).await?;
                    }
                    Message::Close(_) => bail!("bingx closed"),
                    Message::Pong(_) => {
                        last_seen = Instant::now();
                    }
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn handle_text(text: &str, ctx: &SourceContext) -> Result<()> {
    if text == "Ping" || text == "Pong" {
        return Ok(());
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Ok(());
    };
    for event in parse_bingx_events(&value, ctx).await? {
        ctx.emit(event).await?;
    }
    Ok(())
}

async fn parse_bingx_events(value: &Value, ctx: &SourceContext) -> Result<Vec<DataEvent>> {
    let data_type = value
        .get("dataType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value.get("data").unwrap_or(value);
    let symbol = first_str(data, &["s", "symbol"]).unwrap_or_else(|| {
        data_type
            .split('@')
            .next()
            .filter(|x| !x.is_empty())
            .unwrap_or("UNKNOWN")
    });
    if data_type.contains("ticker") {
        let bid = first_str(data, &["bidPrice", "bid", "b"]).unwrap_or("0");
        let ask = first_str(data, &["askPrice", "ask", "a"]).unwrap_or("0");
        emit_tick_ext(
            ctx,
            "bingx",
            MarketKind::Perp,
            symbol,
            bid,
            ask,
            first_str(data, &["markPrice", "mark"]),
            first_str(data, &["fundingRate"]),
            data.get("E").and_then(Value::as_u64),
        )
        .await?;
        if let Some(funding_rate) = first_str(data, &["fundingRate"]).and_then(parse_f64) {
            ctx.emit(DataEvent::FundingRate(FundingRateTick {
                exchange: "bingx",
                symbol: symbol.to_string().into_boxed_str(),
                funding_rate,
                next_funding_time_ms: data.get("nextFundingTime").and_then(Value::as_u64),
                mark_price: first_str(data, &["markPrice", "mark"]).and_then(parse_f64),
                index_price: first_str(data, &["indexPrice"]).and_then(parse_f64),
                ts_ms: data.get("E").and_then(Value::as_u64).unwrap_or_else(now_ms),
            }))
            .await?;
        }
        if let Some(open_interest) = first_str(data, &["openInterest"]).and_then(parse_f64) {
            ctx.emit(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "bingx",
                symbol: symbol.to_string().into_boxed_str(),
                open_interest,
                open_interest_value: first_str(data, &["openInterestValue"]).and_then(parse_f64),
                ts_ms: data.get("E").and_then(Value::as_u64).unwrap_or_else(now_ms),
            }))
            .await?;
        }
        return Ok(Vec::new());
    }
    if data_type.contains("depth") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange: "bingx",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bids: data
                .get("bids")
                .or_else(|| data.get("b"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            asks: data
                .get("asks")
                .or_else(|| data.get("a"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            last_update_id: data.get("u").and_then(Value::as_u64),
            ts_ms: data.get("E").and_then(Value::as_u64).unwrap_or_else(now_ms),
        })]);
    }
    if data_type.contains("trade") {
        let items = data.as_array().into_iter().flatten();
        return Ok(items
            .filter_map(|item| {
                Some(DataEvent::Trade(TradeTick {
                    exchange: "bingx",
                    market: MarketKind::Perp,
                    symbol: symbol.to_string().into_boxed_str(),
                    price: first_str(item, &["p", "price"])?.parse::<f64>().ok()?,
                    qty: first_str(item, &["q", "qty", "quantity"])?
                        .parse::<f64>()
                        .ok()?,
                    side: side_from_str(first_str(item, &["m", "side"]).unwrap_or_default()),
                    trade_id: first_str(item, &["t", "id"]).map(|x| x.to_string().into_boxed_str()),
                    ts_ms: item.get("T").and_then(Value::as_u64).unwrap_or_else(now_ms),
                }))
            })
            .collect());
    }
    Ok(Vec::new())
}

fn bingx_payload(value: &Value) -> &Value {
    value
        .get("data")
        .and_then(|data| {
            data.as_array()
                .and_then(|items| items.first())
                .or(Some(data))
        })
        .unwrap_or(value)
}

fn parse_bingx_funding(symbol: &str, value: &Value) -> Option<DataEvent> {
    let data = bingx_payload(value);
    let funding_rate = first_str(data, &["lastFundingRate", "fundingRate"])?
        .parse()
        .ok()?;
    Some(DataEvent::FundingRate(FundingRateTick {
        exchange: "bingx",
        symbol: first_str(data, &["symbol"])
            .unwrap_or(symbol)
            .to_string()
            .into_boxed_str(),
        funding_rate,
        next_funding_time_ms: data
            .get("nextFundingTime")
            .and_then(Value::as_u64)
            .or_else(|| first_str(data, &["nextFundingTime"]).and_then(|ts| ts.parse().ok())),
        mark_price: first_str(data, &["markPrice", "mark"]).and_then(parse_f64),
        index_price: first_str(data, &["indexPrice"]).and_then(parse_f64),
        ts_ms: now_ms(),
    }))
}

fn parse_bingx_open_interest(symbol: &str, value: &Value) -> Option<DataEvent> {
    let data = bingx_payload(value);
    let open_interest = first_str(data, &["openInterest"])?.parse().ok()?;
    Some(DataEvent::OpenInterest(OpenInterestTick {
        exchange: "bingx",
        symbol: first_str(data, &["symbol"])
            .unwrap_or(symbol)
            .to_string()
            .into_boxed_str(),
        open_interest,
        open_interest_value: Some(open_interest),
        ts_ms: data
            .get("time")
            .and_then(Value::as_u64)
            .or_else(|| data.get("timestamp").and_then(Value::as_u64))
            .or_else(|| first_str(data, &["time", "timestamp"]).and_then(|ts| ts.parse().ok()))
            .unwrap_or_else(now_ms),
    }))
}

fn decode_gzip_text(bytes: &[u8]) -> Option<String> {
    let mut decoder = GzDecoder::new(bytes);
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["false", "buy"], &["true", "sell"])
}

#[cfg(test)]
mod tests {
    use super::{parse_bingx_funding, parse_bingx_open_interest, side_from_str};
    use crate::types::DataEvent;
    use crate::types::TradeSide;
    use serde_json::json;

    #[test]
    fn bingx_side_parser_accepts_common_labels() {
        assert_eq!(side_from_str("false"), TradeSide::Buy);
        assert_eq!(side_from_str("true"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }

    #[test]
    fn bingx_funding_parser_accepts_premium_index_payload() {
        let event = parse_bingx_funding(
            "BTC-USDT",
            &json!({"code":0,"data":{"symbol":"BTC-USDT","markPrice":"100.5","indexPrice":"100.0","lastFundingRate":"0.0001","nextFundingTime":1672041600000_u64}}),
        )
        .expect("funding event");
        match event {
            DataEvent::FundingRate(tick) => {
                assert_eq!(tick.symbol.as_ref(), "BTC-USDT");
                assert_eq!(tick.funding_rate, 0.0001);
                assert_eq!(tick.mark_price, Some(100.5));
                assert_eq!(tick.index_price, Some(100.0));
                assert_eq!(tick.next_funding_time_ms, Some(1672041600000));
            }
            _ => panic!("unexpected event type"),
        }
    }

    #[test]
    fn bingx_open_interest_parser_accepts_linear_payload() {
        let event = parse_bingx_open_interest(
            "BTC-USDT",
            &json!({"code":0,"data":{"openInterest":"3289641547.10","symbol":"BTC-USDT","time":1672026617364_u64}}),
        )
        .expect("oi event");
        match event {
            DataEvent::OpenInterest(tick) => {
                assert_eq!(tick.symbol.as_ref(), "BTC-USDT");
                assert_eq!(tick.open_interest, 3289641547.10);
                assert_eq!(tick.open_interest_value, Some(3289641547.10));
                assert_eq!(tick.ts_ms, 1672026617364);
            }
            _ => panic!("unexpected event type"),
        }
    }
}
