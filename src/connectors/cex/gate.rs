use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{
    emit_tick, emit_tick_ext, parse_array_levels, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

// ── Shared types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GateMsg {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    result: Option<GateResult>,
}

#[derive(Deserialize)]
struct GateResult {
    #[serde(default)]
    s: Option<String>,
    #[serde(default)]
    b: Option<String>,
    #[serde(default)]
    a: Option<String>,
    #[serde(default)]
    contract: Option<String>,
    #[serde(default)]
    highest_bid: Option<String>,
    #[serde(default)]
    lowest_ask: Option<String>,
    #[serde(default)]
    mark_price: Option<String>,
    #[serde(default)]
    funding_rate: Option<String>,
    #[serde(default)]
    time_ms: Option<u64>,
}

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_gate(
    url: &str,
    channel: &str,
    ping_channel: &str,
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
        anyhow::bail!("gate {label} symbols empty");
    }

    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({"time":now_ms()/1000,"channel":channel,"event":"subscribe","payload":symbols})
            .to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    anyhow::bail!("gate {label} heartbeat timeout");
                }
                sink.send(Message::Text(json!({"time":now_ms()/1000,"channel":ping_channel}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("gate {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        last_seen = Instant::now();
                        if let Ok(v) = serde_json::from_str::<GateMsg>(&t)
                            && v.channel.as_deref() == Some(channel)
                            && let Some(r) = v.result
                        {
                            match market {
                                MarketKind::Spot => {
                                    if let (Some(symbol), Some(bid), Some(ask)) =
                                        (r.s.as_deref(), r.b.as_deref(), r.a.as_deref())
                                    {
                                        emit_tick(&ctx, exchange, market, symbol, bid, ask).await?;
                                    }
                                }
                                MarketKind::Perp => {
                                    if let (Some(symbol), Some(bid), Some(ask)) =
                                        (r.contract.as_deref(), r.highest_bid.as_deref(), r.lowest_ask.as_deref())
                                    {
                                        emit_tick_ext(
                                            &ctx, exchange, market, symbol, bid, ask,
                                            r.mark_price.as_deref(), r.funding_rate.as_deref(), r.time_ms,
                                        ).await?;
                                    }
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("gate {label} closed"),
                }
            }
        }
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct GateSpotBookTicker {
    pub symbols: Vec<String>,
}
impl GateSpotBookTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for GateSpotBookTicker {
    fn name(&self) -> &'static str {
        "gate"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_gate(
            "wss://api.gateio.ws/ws/v4/",
            "spot.book_ticker",
            "spot.ping",
            self.name(),
            MarketKind::Spot,
            &self.symbols,
            ctx,
        )
        .await
    }
}

pub struct GateSpotRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl GateSpotRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for GateSpotRestFeed {
    fn name(&self) -> &'static str {
        "gate"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("gate spot REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_gate_spot_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "gate",
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

async fn poll_gate_spot_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get("https://api.gateio.ws/api/v4/spot/order_book")
        .query(&[("currency_pair", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_book(symbol, &book));

    let trades = client
        .get("https://api.gateio.ws/api/v4/spot/trades")
        .query(&[("currency_pair", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_trades(symbol, &trades));

    Ok(events)
}

fn parse_gate_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = value
        .get("current")
        .or_else(|| value.get("update"))
        .and_then(parse_value_f64)
        .map(seconds_or_millis)
        .unwrap_or_else(now_ms);
    let normalized = symbol.to_ascii_uppercase();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "gate",
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
        exchange: "gate",
        market: MarketKind::Spot,
        symbol: normalized.into_boxed_str(),
        bids,
        asks,
        last_update_id: value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<u64>().ok()),
        ts_ms,
    }));

    events
}

fn parse_gate_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "gate",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("create_time_ms")
                    .or_else(|| trade.get("create_time"))
                    .and_then(parse_value_f64)
                    .map(seconds_or_millis)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn seconds_or_millis(ts: f64) -> u64 {
    if ts < 10_000_000_000.0 {
        (ts * 1_000.0) as u64
    } else {
        ts as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_gate_spot_book_and_trades() {
        let book = parse_gate_book(
            "BTC_USDT",
            &json!({
                "id": "12345",
                "current": 1779297822.639_f64,
                "bids": [["77527", "0.42197508"]],
                "asks": [["77527.1", "0.33297863"]]
            }),
        );
        let trades = parse_gate_trades(
            "BTC_USDT",
            &json!([{
                "id": "987",
                "price": "77510.6",
                "amount": "0.00001",
                "side": "buy",
                "create_time_ms": "1779297769043"
            }]),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.ts_ms, 1_779_297_769_043);
    }
}
