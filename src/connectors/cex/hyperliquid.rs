use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_object_levels, side_from_labels};
use crate::connectors::cex::ws::run_reconnecting;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick, TradeSide, TradeTick,
    now_ms,
};

pub struct HyperliquidFeed {
    coins: Vec<String>,
}

impl HyperliquidFeed {
    pub fn new(coins: Vec<String>) -> Self {
        Self { coins }
    }
}

#[async_trait]
impl ExchangeSource for HyperliquidFeed {
    fn name(&self) -> &'static str {
        "hyperliquid"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.coins.is_empty() {
            bail!("hyperliquid coins empty");
        }
        let coins = self.coins.clone();
        run_reconnecting("hyperliquid", move || {
            let coins = coins.clone();
            let ctx = ctx.clone();
            async move { run_hyperliquid_once(&coins, ctx).await }
        })
        .await
    }
}

async fn run_hyperliquid_once(coins: &[String], ctx: SourceContext) -> Result<()> {
    let (ws, _) = connect_async("wss://api.hyperliquid.xyz/ws")
        .await
        .context("hyperliquid connect failed")?;
    let (mut sink, mut stream) = ws.split();
    for coin in coins {
        for sub_type in ["l2Book", "trades", "activeAssetCtx"] {
            sink.send(Message::Text(
                json!({"method":"subscribe","subscription":{"type":sub_type,"coin":coin}})
                    .to_string(),
            ))
            .await?;
        }
    }
    let mut ping = interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "hyperliquid", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("hyperliquid stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_hyperliquid_events(&value) {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("hyperliquid closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

fn parse_hyperliquid_events(value: &Value) -> Vec<DataEvent> {
    let channel = value
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value.get("data").unwrap_or(value);
    match channel {
        "l2Book" => parse_l2_book(data).into_iter().collect(),
        "trades" => parse_trades(data),
        "activeAssetCtx" => parse_active_asset_ctx(data).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn parse_l2_book(data: &Value) -> Option<DataEvent> {
    let coin = data.get("coin").and_then(Value::as_str)?;
    let levels = data.get("levels").and_then(Value::as_array)?;
    let bids = levels
        .first()
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "px", "sz"))
        .unwrap_or_default();
    let asks = levels
        .get(1)
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "px", "sz"))
        .unwrap_or_default();
    Some(DataEvent::OrderBook(OrderBookTick {
        exchange: "hyperliquid",
        market: MarketKind::Perp,
        symbol: coin.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms: data
            .get("time")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms),
    }))
}

fn parse_trades(data: &Value) -> Vec<DataEvent> {
    data.as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let coin = item.get("coin").and_then(Value::as_str)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "hyperliquid",
                market: MarketKind::Perp,
                symbol: coin.to_string().into_boxed_str(),
                price: item.get("px")?.as_str()?.parse::<f64>().ok()?,
                qty: item.get("sz")?.as_str()?.parse::<f64>().ok()?,
                side: side_from_str(item.get("side").and_then(Value::as_str).unwrap_or_default()),
                trade_id: item
                    .get("hash")
                    .and_then(Value::as_str)
                    .map(|x| x.to_string().into_boxed_str()),
                ts_ms: item
                    .get("time")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_active_asset_ctx(data: &Value) -> Option<DataEvent> {
    let coin = data.get("coin").and_then(Value::as_str)?;
    let ctx = data.get("ctx").unwrap_or(data);
    if let Some(funding_rate) = ctx
        .get("funding")
        .and_then(Value::as_str)
        .and_then(|x| x.parse::<f64>().ok())
    {
        return Some(DataEvent::FundingRate(FundingRateTick {
            exchange: "hyperliquid",
            symbol: coin.to_string().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: None,
            mark_price: ctx
                .get("markPx")
                .and_then(Value::as_str)
                .and_then(|x| x.parse::<f64>().ok()),
            index_price: ctx
                .get("oraclePx")
                .and_then(Value::as_str)
                .and_then(|x| x.parse::<f64>().ok()),
            ts_ms: now_ms(),
        }));
    }
    ctx.get("openInterest")
        .and_then(Value::as_str)
        .and_then(|x| x.parse::<f64>().ok())
        .map(|open_interest| {
            DataEvent::OpenInterest(OpenInterestTick {
                exchange: "hyperliquid",
                symbol: coin.to_string().into_boxed_str(),
                open_interest,
                open_interest_value: None,
                ts_ms: now_ms(),
            })
        })
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["b", "buy"], &["a", "sell"])
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn hyperliquid_side_parser_accepts_wire_labels() {
        assert_eq!(side_from_str("B"), TradeSide::Buy);
        assert_eq!(side_from_str("A"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }
}
