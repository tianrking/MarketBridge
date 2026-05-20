use async_trait::async_trait;
use std::io::{Cursor, Read};
use std::time::Duration;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{
    emit_tick_f64, parse_array_levels, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_htx(
    url: &str,
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
        anyhow::bail!("htx {label} symbols empty");
    }

    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();

    for s in symbols {
        let ch = format!("market.{}.bbo", s.to_ascii_lowercase());
        sink.send(Message::Text(json!({"sub": ch, "id": s}).to_string()))
            .await?;
    }

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    anyhow::bail!("htx {label} heartbeat timeout");
                }
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("htx {label} stream ended"))??;
                match msg {
                    Message::Binary(bin) => {
                        let mut d = GzDecoder::new(Cursor::new(bin));
                        let mut s = String::new();
                        d.read_to_string(&mut s)?;
                        last_seen = Instant::now();
                        if let Ok(v) = serde_json::from_str::<Value>(&s) {
                            if let Some(ping) = v.get("ping").and_then(|x| x.as_i64()) {
                                sink.send(Message::Text(json!({"pong": ping}).to_string())).await?;
                                continue;
                            }
                            let ch = v.get("ch").and_then(|x| x.as_str()).unwrap_or("");
                            let symbol = ch.split('.').nth(1).unwrap_or("UNKNOWN");
                            let bid = v.pointer("/tick/bid/0").and_then(|x| x.as_f64());
                            let ask = v.pointer("/tick/ask/0").and_then(|x| x.as_f64());
                            let ts = v.pointer("/tick/ts").and_then(|x| x.as_u64()).or_else(|| v.get("ts").and_then(|x| x.as_u64()));
                            if let (Some(bid), Some(ask)) = (bid, ask) {
                                emit_tick_f64(&ctx, exchange, market, symbol, bid, ask, None, None, ts).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Text(_) | Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("htx {label} closed"),
                }
            }
        }
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct HtxBbo {
    pub symbols: Vec<String>,
}
impl HtxBbo {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for HtxBbo {
    fn name(&self) -> &'static str {
        "htx"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_htx(
            "wss://api.huobi.pro/ws",
            self.name(),
            MarketKind::Spot,
            &self.symbols,
            ctx,
        )
        .await
    }
}

pub struct HtxSpotRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl HtxSpotRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for HtxSpotRestFeed {
    fn name(&self) -> &'static str {
        "htx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("htx spot REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_htx_spot_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "htx",
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

async fn poll_htx_spot_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let rest_symbol = symbol.to_ascii_lowercase();
    let mut events = Vec::new();

    let book = client
        .get("https://api.huobi.pro/market/depth")
        .query(&[
            ("symbol", rest_symbol.as_str()),
            ("type", "step0"),
            ("depth", "20"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_book(symbol, &book));

    let trades = client
        .get("https://api.huobi.pro/market/history/trade")
        .query(&[("symbol", rest_symbol.as_str()), ("size", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_trades(symbol, &trades));

    Ok(events)
}

fn parse_htx_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("tick").unwrap_or(value);
    let bids = row
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = row
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = row
        .get("ts")
        .or_else(|| value.get("ts"))
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let normalized = symbol.to_ascii_uppercase();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "htx",
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
        exchange: "htx",
        market: MarketKind::Spot,
        symbol: normalized.into_boxed_str(),
        bids,
        asks,
        last_update_id: row.get("version").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_htx_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .flat_map(|batch| {
            batch
                .get("data")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
        })
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "htx",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("direction")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("id")
                    .and_then(Value::as_i64)
                    .map(|id| id.to_string().into_boxed_str()),
                ts_ms: trade
                    .get("ts")
                    .and_then(parse_value_f64)
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
    fn parses_htx_spot_book_and_trades() {
        let book = parse_htx_book(
            "BTCUSDT",
            &json!({"tick": {
                "version": 12345_u64,
                "ts": 1779297822639_u64,
                "bids": [[77527.0, 0.42197508]],
                "asks": [[77527.1, 0.33297863]]
            }}),
        );
        let trades = parse_htx_trades(
            "BTCUSDT",
            &json!({"data": [{"data": [{
                "id": 987_i64,
                "price": 77510.6,
                "amount": 0.00001,
                "direction": "buy",
                "ts": 1779297769043_u64
            }]}]}),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.ts_ms, 1_779_297_769_043);
    }
}
