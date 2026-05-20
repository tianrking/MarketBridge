use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{emit_tick_ext, parse_array_levels};
use crate::connectors::cex::ws::run_reconnecting;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, OrderBookTick, TradeSide, TradeTick, now_ms};

pub struct BackpackFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl BackpackFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

#[async_trait]
impl ExchangeSource for BackpackFeed {
    fn name(&self) -> &'static str {
        "backpack"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            bail!("backpack symbols empty");
        }
        let market = self.market;
        let symbols = self.symbols.clone();
        run_reconnecting("backpack", move || {
            let symbols = symbols.clone();
            let ctx = ctx.clone();
            async move { run_backpack_once(market, &symbols, ctx).await }
        })
        .await
    }
}

async fn run_backpack_once(
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let (ws, _) = connect_async("wss://ws.backpack.exchange")
        .await
        .context("backpack connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let streams = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("bookTicker.{symbol}"),
                format!("depth.{symbol}"),
                format!("trade.{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"method":"SUBSCRIBE","params":streams}).to_string(),
    ))
    .await?;
    let mut ping = interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "backpack", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("backpack stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_backpack_events(market, &value, &ctx).await? {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("backpack closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn parse_backpack_events(
    market: MarketKind,
    value: &Value,
    ctx: &SourceContext,
) -> Result<Vec<DataEvent>> {
    let stream = value
        .get("stream")
        .or_else(|| value.get("e"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value.get("data").unwrap_or(value);
    let symbol = data
        .get("s")
        .or_else(|| data.get("symbol"))
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if stream.contains("bookTicker") {
        let bid = data
            .get("b")
            .or_else(|| data.get("bidPrice"))
            .and_then(Value::as_str)
            .unwrap_or("0");
        let ask = data
            .get("a")
            .or_else(|| data.get("askPrice"))
            .and_then(Value::as_str)
            .unwrap_or("0");
        emit_tick_ext(ctx, "backpack", market, symbol, bid, ask, None, None, None).await?;
        return Ok(Vec::new());
    }
    if stream.contains("depth") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange: "backpack",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bids: data
                .get("b")
                .or_else(|| data.get("bids"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            asks: data
                .get("a")
                .or_else(|| data.get("asks"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            last_update_id: data.get("u").and_then(Value::as_u64),
            ts_ms: data.get("E").and_then(Value::as_u64).unwrap_or_else(now_ms),
        })]);
    }
    if stream.contains("trade") {
        let Some(price) = string_f64(data, "p").or_else(|| string_f64(data, "price")) else {
            return Ok(Vec::new());
        };
        let Some(qty) = string_f64(data, "q").or_else(|| string_f64(data, "quantity")) else {
            return Ok(Vec::new());
        };
        return Ok(vec![DataEvent::Trade(TradeTick {
            exchange: "backpack",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            price,
            qty,
            side: side_from_str(data.get("m").and_then(Value::as_bool)),
            trade_id: data
                .get("t")
                .or_else(|| data.get("tradeId"))
                .and_then(|x| {
                    x.as_i64()
                        .map(|n| n.to_string())
                        .or_else(|| x.as_str().map(str::to_string))
                })
                .map(String::into_boxed_str),
            ts_ms: data.get("T").and_then(Value::as_u64).unwrap_or_else(now_ms),
        })]);
    }
    Ok(Vec::new())
}

fn string_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_str()?.parse::<f64>().ok()
}

fn side_from_str(buyer_is_maker: Option<bool>) -> TradeSide {
    match buyer_is_maker {
        Some(true) => TradeSide::Sell,
        Some(false) => TradeSide::Buy,
        None => TradeSide::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn backpack_side_parser_accepts_maker_flag() {
        assert_eq!(side_from_str(Some(false)), TradeSide::Buy);
        assert_eq!(side_from_str(Some(true)), TradeSide::Sell);
        assert_eq!(side_from_str(None), TradeSide::Unknown);
    }
}
