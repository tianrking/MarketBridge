use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{
    emit_tick, parse_array_levels, parse_exchange_datetime_ms, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const COINBASE_EXCHANGE_REST_URL: &str = "https://api.exchange.coinbase.com";

pub struct CoinbaseTicker {
    pub product_ids: Vec<String>,
}
impl CoinbaseTicker {
    pub fn new(product_ids: Vec<String>) -> Self {
        Self { product_ids }
    }
}

pub struct CoinbaseRestFeed {
    product_ids: Vec<String>,
    client: reqwest::Client,
}

impl CoinbaseRestFeed {
    pub fn new(product_ids: Vec<String>) -> Self {
        Self {
            product_ids,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Deserialize)]
struct CbMsg {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    best_bid: Option<String>,
    #[serde(default)]
    best_ask: Option<String>,
    #[serde(default)]
    events: Vec<CbEvent>,
}

#[derive(Deserialize)]
struct CbEvent {
    #[serde(default)]
    tickers: Vec<CbTicker>,
}

#[derive(Deserialize)]
struct CbTicker {
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    best_bid: Option<String>,
    #[serde(default)]
    best_ask: Option<String>,
}

#[async_trait]
impl ExchangeSource for CoinbaseTicker {
    fn name(&self) -> &'static str {
        "coinbase"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.product_ids.is_empty() {
            anyhow::bail!("coinbase product_ids empty");
        }

        let (ws, _) = connect_async("wss://advanced-trade-ws.coinbase.com").await?;
        let (mut sink, mut stream) = ws.split();

        sink.send(Message::Text(
            json!({
                "type":"subscribe",
                "channel":"ticker",
                "product_ids": self.product_ids
            })
            .to_string(),
        ))
        .await?;

        let mut ping_tick = interval(Duration::from_secs(20));
        let mut last_pong = Instant::now();
        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_pong.elapsed() > Duration::from_secs(90) { anyhow::bail!("coinbase heartbeat timeout"); }
                    sink.send(Message::Text(json!({"type":"ping"}).to_string())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: self.name(), ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("coinbase stream ended")??;
                    match msg {
                        Message::Text(t) => {
                            if let Ok(v) = serde_json::from_str::<CbMsg>(&t) {
                                if v.r#type.as_deref() == Some("pong") {
                                    last_pong = Instant::now();
                                    continue;
                                }

                                if let (Some(symbol), Some(bid), Some(ask)) = (v.product_id.as_deref(), v.best_bid.as_deref(), v.best_ask.as_deref()) {
                                    emit_tick(&ctx, self.name(), MarketKind::Spot, symbol, bid, ask).await?;
                                }

                                for e in v.events {
                                    for t in e.tickers {
                                        if let (Some(symbol), Some(bid), Some(ask)) = (t.product_id.as_deref(), t.best_bid.as_deref(), t.best_ask.as_deref()) {
                                            emit_tick(&ctx, self.name(), MarketKind::Spot, symbol, bid, ask).await?;
                                        }
                                    }
                                }
                            }
                        }
                        Message::Pong(_) => last_pong = Instant::now(),
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Close(_) => anyhow::bail!("coinbase closed"),
                        Message::Binary(_) | Message::Frame(_) => {}
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinbaseRestFeed {
    fn name(&self) -> &'static str {
        "coinbase"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.product_ids.is_empty() {
            anyhow::bail!("coinbase product_ids empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for product_id in &self.product_ids {
                match poll_coinbase_rest(&self.client, product_id).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => tracing::warn!(
                        exchange = "coinbase",
                        symbol = product_id,
                        error = %err,
                        "poll failed"
                    ),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: self.name(),
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_coinbase_rest(client: &reqwest::Client, product_id: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!(
            "{COINBASE_EXCHANGE_REST_URL}/products/{product_id}/book"
        ))
        .query(&[("level", "2")])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    events.extend(parse_coinbase_book(product_id, &book));

    let trades = client
        .get(format!(
            "{COINBASE_EXCHANGE_REST_URL}/products/{product_id}/trades"
        ))
        .query(&[("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    events.extend(parse_coinbase_trades(product_id, &trades));

    Ok(events)
}

fn parse_coinbase_book(product_id: &str, value: &serde_json::Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(serde_json::Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(serde_json::Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "coinbase",
            market: MarketKind::Spot,
            symbol: product_id.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "coinbase",
        market: MarketKind::Spot,
        symbol: product_id.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("sequence").and_then(serde_json::Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_coinbase_trades(product_id: &str, value: &serde_json::Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "coinbase",
                market: MarketKind::Spot,
                symbol: product_id.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("size").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(serde_json::Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("trade_id")
                    .and_then(|value| {
                        value
                            .as_u64()
                            .map(|id| id.to_string())
                            .or_else(|| value.as_str().map(str::to_string))
                    })
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("time")
                    .and_then(serde_json::Value::as_str)
                    .and_then(parse_exchange_datetime_ms)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_coinbase_exchange_book_and_trades() {
        let book = parse_coinbase_book(
            "BTC-USDT",
            &json!({
                "sequence": 100_u64,
                "bids": [["77372.09", "0.002211", 1]],
                "asks": [["77380.00", "0.003", 1]]
            }),
        );
        let trades = parse_coinbase_trades(
            "BTC-USDT",
            &json!([{
                "trade_id": 35358895_u64,
                "side": "sell",
                "size": "0.00058839",
                "price": "77433.29000000",
                "time": "2026-05-20T17:13:40.684649Z"
            }]),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.trade_id.as_deref(), Some("35358895"));
        assert_eq!(trade.ts_ms, 1_779_297_220_000);
    }
}
