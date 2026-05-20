use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{
    emit_tick_ext, first_str, parse_array_levels, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, OrderBookTick, TradeSide, TradeTick, now_ms};

pub struct MexcFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl MexcFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

#[async_trait]
impl ExchangeSource for MexcFeed {
    fn name(&self) -> &'static str {
        "mexc"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        match self.market {
            MarketKind::Spot => run_mexc_spot(&self.symbols, ctx).await,
            MarketKind::Perp => run_mexc_contract(&self.symbols, ctx).await,
        }
    }
}

async fn run_mexc_spot(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("mexc spot symbols empty");
    }
    let (ws, _) = connect_async("wss://wbs.mexc.com/ws")
        .await
        .context("mexc spot connect failed")?;
    let (mut sink, stream) = ws.split();
    let params = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("spot@public.bookTicker.v3.api@{symbol}"),
                format!("spot@public.limit.depth.v3.api@{symbol}@20"),
                format!("spot@public.deals.v3.api@{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"method":"SUBSCRIPTION","params":params})
            .to_string()
            .into(),
    ))
    .await?;
    run_json_loop("mexc", MarketKind::Spot, sink, stream, ctx).await
}

async fn run_mexc_contract(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("mexc perp symbols empty");
    }
    let (ws, _) = connect_async("wss://contract.mexc.com/edge")
        .await
        .context("mexc contract connect failed")?;
    let (mut sink, stream) = ws.split();
    for symbol in symbols {
        for method in ["sub.ticker", "sub.depth.full", "sub.deal"] {
            sink.send(Message::Text(
                json!({"method":method,"param":{"symbol":symbol}})
                    .to_string()
                    .into(),
            ))
            .await?;
        }
    }
    run_json_loop("mexc", MarketKind::Perp, sink, stream, ctx).await
}

async fn run_json_loop<S>(
    exchange: &'static str,
    market: MarketKind,
    mut sink: S,
    mut stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    ctx: SourceContext,
) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    let mut ping = interval(Duration::from_secs(20));
    loop {
        tokio::select! {
            _ = ping.tick() => {
                sink.send(Message::Text(json!({"method":"PING"}).to_string().into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("mexc stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_mexc_events(market, &value, &ctx).await? {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("mexc closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn parse_mexc_events(
    market: MarketKind,
    value: &Value,
    ctx: &SourceContext,
) -> Result<Vec<DataEvent>> {
    let channel = value
        .get("c")
        .or_else(|| value.get("channel"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value
        .get("d")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    let symbol = value
        .get("s")
        .or_else(|| value.get("symbol"))
        .or_else(|| data.get("symbol"))
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if channel.contains("bookTicker") || channel.contains("ticker") {
        let bid = first_str(data, &["bidPrice", "bid", "b", "bid1"]).unwrap_or("0");
        let ask = first_str(data, &["askPrice", "ask", "a", "ask1"]).unwrap_or("0");
        emit_tick_ext(
            ctx,
            "mexc",
            market,
            symbol,
            bid,
            ask,
            None,
            first_str(data, &["fundingRate"]),
            None,
        )
        .await?;
        return Ok(Vec::new());
    }
    if channel.contains("depth") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange: "mexc",
            market,
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
            last_update_id: None,
            ts_ms: value
                .get("t")
                .and_then(Value::as_u64)
                .unwrap_or_else(now_ms),
        })]);
    }
    if channel.contains("deal") || channel.contains("trade") {
        let items = data
            .get("deals")
            .and_then(Value::as_array)
            .or_else(|| data.as_array());
        return Ok(items
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some(DataEvent::Trade(TradeTick {
                    exchange: "mexc",
                    market,
                    symbol: symbol.to_string().into_boxed_str(),
                    price: first_str(item, &["p", "price"])?.parse::<f64>().ok()?,
                    qty: first_str(item, &["v", "q", "quantity"])?
                        .parse::<f64>()
                        .ok()?,
                    side: side_from_str(first_str(item, &["S", "T", "side"]).unwrap_or_default()),
                    trade_id: first_str(item, &["t", "id"]).map(|x| x.to_string().into_boxed_str()),
                    ts_ms: item.get("t").and_then(Value::as_u64).unwrap_or_else(now_ms),
                }))
            })
            .collect());
    }
    Ok(Vec::new())
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["1", "buy"], &["2", "sell"])
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn mexc_side_parser_accepts_common_labels() {
        assert_eq!(side_from_str("1"), TradeSide::Buy);
        assert_eq!(side_from_str("2"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }
}
