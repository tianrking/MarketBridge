use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{
    emit_tick, emit_tick_ext, parse_array_levels, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const KRAKEN_REST_URL: &str = "https://api.kraken.com/0/public";

// ── Shared types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct KMsg {
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    data: Vec<KData>,
}

#[derive(Deserialize)]
struct KData {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    bid: Option<String>,
    #[serde(default)]
    ask: Option<String>,
    #[serde(default)]
    mark: Option<String>,
    #[serde(default)]
    funding_rate: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
}

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_kraken(
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
        anyhow::bail!("kraken {label} symbols empty");
    }

    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({"method":"subscribe","params":{"channel":"ticker","symbol":symbols}}).to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(90) {
                    anyhow::bail!("kraken {label} heartbeat timeout");
                }
                sink.send(Message::Text(json!({"method":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("kraken {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        if let Ok(v) = serde_json::from_str::<KMsg>(&t) {
                            if v.method.as_deref() == Some("pong") {
                                last_pong = Instant::now();
                                continue;
                            }
                            if v.channel.as_deref() == Some("ticker") {
                                for d in v.data {
                                    if let (Some(symbol), Some(bid), Some(ask)) =
                                        (d.symbol.as_deref(), d.bid.as_deref(), d.ask.as_deref())
                                    {
                                        match market {
                                            MarketKind::Spot => {
                                                emit_tick(&ctx, exchange, market, symbol, bid, ask).await?;
                                            }
                                            MarketKind::Perp => {
                                                emit_tick_ext(
                                                    &ctx, exchange, market, symbol, bid, ask,
                                                    d.mark.as_deref(), d.funding_rate.as_deref(),
                                                    d.timestamp.as_deref().and_then(|x| x.parse::<u64>().ok()),
                                                ).await?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Message::Pong(_) => last_pong = Instant::now(),
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => anyhow::bail!("kraken {label} closed"),
                    Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct KrakenTicker {
    pub symbols: Vec<String>,
}
impl KrakenTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct KrakenRestFeed {
    pairs: Vec<String>,
    client: reqwest::Client,
}

impl KrakenRestFeed {
    pub fn new(pairs: Vec<String>) -> Self {
        Self {
            pairs,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for KrakenTicker {
    fn name(&self) -> &'static str {
        "kraken"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_kraken(
            "wss://ws.kraken.com/v2",
            self.name(),
            MarketKind::Spot,
            &self.symbols,
            ctx,
        )
        .await
    }
}

#[async_trait]
impl ExchangeSource for KrakenRestFeed {
    fn name(&self) -> &'static str {
        "kraken"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.pairs.is_empty() {
            anyhow::bail!("kraken spot REST pairs empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for pair in &self.pairs {
                match poll_kraken_spot_rest(&self.client, pair).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => tracing::warn!(
                        exchange = "kraken",
                        symbol = pair,
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

async fn poll_kraken_spot_rest(client: &reqwest::Client, pair: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("{KRAKEN_REST_URL}/Depth"))
        .query(&[("pair", pair), ("count", "15")])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    events.extend(parse_kraken_book(pair, &book));

    let trades = client
        .get(format!("{KRAKEN_REST_URL}/Trades"))
        .query(&[("pair", pair), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    events.extend(parse_kraken_trades(pair, &trades));

    Ok(events)
}

fn kraken_result_market<'a>(
    fallback_pair: &'a str,
    value: &'a serde_json::Value,
) -> Option<(&'a str, &'a serde_json::Value)> {
    value
        .get("result")?
        .as_object()?
        .iter()
        .find(|(key, _)| key.as_str() != "last")
        .map(|(key, row)| (key.as_str(), row))
        .or(Some((fallback_pair, value)))
}

fn parse_kraken_book(pair: &str, value: &serde_json::Value) -> Vec<DataEvent> {
    let Some((symbol, row)) = kraken_result_market(pair, value) else {
        return Vec::new();
    };
    let bids = row
        .get("bids")
        .and_then(serde_json::Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = row
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
            exchange: "kraken",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "kraken",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_kraken_trades(pair: &str, value: &serde_json::Value) -> Vec<DataEvent> {
    let Some((symbol, row)) = kraken_result_market(pair, value) else {
        return Vec::new();
    };
    row.as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            let fields = trade.as_array()?;
            let ts_ms = fields
                .get(2)
                .and_then(parse_value_f64)
                .map(|secs| (secs * 1000.0) as u64)
                .unwrap_or_else(now_ms);
            Some(DataEvent::Trade(TradeTick {
                exchange: "kraken",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: fields.first().and_then(parse_value_f64)?,
                qty: fields.get(1).and_then(parse_value_f64)?,
                side: fields
                    .get(3)
                    .and_then(serde_json::Value::as_str)
                    .map(|side| side_from_labels(side, &["b"], &["s"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: fields.get(6).and_then(|value| {
                    value
                        .as_u64()
                        .map(|id| id.to_string().into_boxed_str())
                        .or_else(|| value.as_str().map(|id| id.to_string().into_boxed_str()))
                }),
                ts_ms,
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_kraken_rest_book_and_trades() {
        let book = parse_kraken_book(
            "BTCUSDT",
            &json!({"result": {"XBTUSDT": {
                "asks": [["77491.80000", "0.001", 1779297704]],
                "bids": [["77491.70000", "0.130", 1779297705]]
            }}}),
        );
        let trades = parse_kraken_trades(
            "BTCUSDT",
            &json!({"result": {"XBTUSDT": [[
                "77475.60000", "0.00011000", 1779297610.745688_f64, "b", "l", "", 11320873_u64
            ]], "last": "1779297695624280254"}}),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.trade_id.as_deref(), Some("11320873"));
    }
}
