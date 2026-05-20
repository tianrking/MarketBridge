use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::emit_tick_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_bitfinex(
    exchange: &'static str,
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let label = if market == MarketKind::Spot {
        "spot"
    } else {
        "perp"
    };
    if symbols.is_empty() {
        anyhow::bail!("bitfinex {label} symbols empty");
    }

    let (ws, _) = connect_async("wss://api-pub.bitfinex.com/ws/2").await?;
    let (mut sink, mut stream) = ws.split();

    for sym in symbols {
        sink.send(Message::Text(
            json!({"event":"subscribe","channel":"ticker","symbol":sym}).to_string(),
        ))
        .await?;
    }

    let mut chan_map: HashMap<i64, String> = HashMap::new();
    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    anyhow::bail!("bitfinex {label} heartbeat timeout");
                }
                sink.send(Message::Text(json!({"event":"ping","cid":now_ms()}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("bitfinex {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        last_seen = Instant::now();
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                            if v.is_object() {
                                let chan_id = v.get("chanId").and_then(|x| x.as_i64());
                                let event = v.get("event").and_then(|x| x.as_str());
                                let sym = v.get("symbol").and_then(|x| x.as_str());
                                if event == Some("subscribed")
                                    && let (Some(cid), Some(sym)) = (chan_id, sym)
                                {
                                    chan_map.insert(cid, sym.to_string());
                                }
                                continue;
                            }
                            if let Some(arr) = v.as_array()
                                && arr.len() >= 2
                                && let Some(chan_id) = arr[0].as_i64()
                                && let Some(data) = arr[1].as_array()
                                && data.len() >= 4
                            {
                                let bid = data[0].as_f64();
                                let ask = data[2].as_f64();
                                if let (Some(bid), Some(ask), Some(sym)) = (bid, ask, chan_map.get(&chan_id)) {
                                    emit_tick_f64(&ctx, exchange, market, sym, bid, ask, None, None, None).await?;
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("bitfinex {label} closed"),
                }
            }
        }
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct BitfinexTicker {
    pub symbols: Vec<String>,
}
impl BitfinexTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BitfinexTicker {
    fn name(&self) -> &'static str {
        "bitfinex"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bitfinex(self.name(), MarketKind::Spot, &self.symbols, ctx).await
    }
}

pub struct BitfinexSpotRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitfinexSpotRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitfinexSpotRestFeed {
    fn name(&self) -> &'static str {
        "bitfinex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitfinex spot REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitfinex_spot_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "bitfinex",
                        symbol,
                        error = %error,
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

async fn poll_bitfinex_spot_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("https://api-pub.bitfinex.com/v2/book/{symbol}/P0"))
        .query(&[("len", "25")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitfinex_book(symbol, &book));

    let trades = client
        .get(format!(
            "https://api-pub.bitfinex.com/v2/trades/{symbol}/hist"
        ))
        .query(&[("limit", "25")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitfinex_trades(symbol, &trades));

    Ok(events)
}

fn parse_bitfinex_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    for row in value.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(items) = row.as_array() else {
            continue;
        };
        let (Some(price), Some(count), Some(amount)) = (
            items.first().and_then(Value::as_f64),
            items.get(1).and_then(Value::as_f64),
            items.get(2).and_then(Value::as_f64),
        ) else {
            continue;
        };
        if count <= 0.0 || amount == 0.0 {
            continue;
        }
        let level = BookLevel {
            price,
            qty: amount.abs(),
        };
        if amount > 0.0 {
            bids.push(level);
        } else {
            asks.push(level);
        }
    }

    let normalized = symbol.to_ascii_uppercase();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitfinex",
            market: MarketKind::Spot,
            symbol: normalized.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitfinex",
        market: MarketKind::Spot,
        symbol: normalized.into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_bitfinex_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let items = row.as_array()?;
            let amount = items.get(2).and_then(Value::as_f64)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitfinex",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: items.get(3).and_then(Value::as_f64)?,
                qty: amount.abs(),
                side: if amount > 0.0 {
                    TradeSide::Buy
                } else if amount < 0.0 {
                    TradeSide::Sell
                } else {
                    TradeSide::Unknown
                },
                trade_id: items
                    .first()
                    .and_then(Value::as_i64)
                    .map(|id| id.to_string().into_boxed_str()),
                ts_ms: items
                    .get(1)
                    .and_then(Value::as_f64)
                    .map(|ts| ts as u64)
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
    fn parses_bitfinex_spot_book_and_trades() {
        let book = parse_bitfinex_book(
            "tBTCUSD",
            &json!([[77527.0, 1, 0.42197508], [77527.1, 1, -0.33297863]]),
        );
        let trades = parse_bitfinex_trades(
            "tBTCUSD",
            &json!([[987_i64, 1779297769043_u64, 0.00001, 77510.6]]),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.side, TradeSide::Buy);
        assert_eq!(trade.ts_ms, 1_779_297_769_043);
    }
}
