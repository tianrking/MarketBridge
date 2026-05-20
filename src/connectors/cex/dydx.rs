use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

pub struct DydxFeed {
    markets: Vec<String>,
}

impl DydxFeed {
    pub fn new(markets: Vec<String>) -> Self {
        Self { markets }
    }
}

#[async_trait]
impl ExchangeSource for DydxFeed {
    fn name(&self) -> &'static str {
        "dydx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        let ws_ctx = ctx.clone();
        let markets_ctx = ctx.clone();
        let markets = self.markets.clone();
        let markets_for_rest = self.markets.clone();
        tokio::try_join!(
            run_dydx_ws(&markets, ws_ctx),
            run_dydx_markets_poll(&markets_for_rest, markets_ctx),
        )?;
        Ok(())
    }
}

async fn run_dydx_ws(markets: &[String], ctx: SourceContext) -> Result<()> {
    if markets.is_empty() {
        bail!("dydx markets empty");
    }
    let (ws, _) = connect_async("wss://indexer.dydx.trade/v4/ws")
        .await
        .context("dydx connect failed")?;
    let (mut sink, mut stream) = ws.split();
    for market in markets {
        for channel in ["v4_orderbook", "v4_trades"] {
            sink.send(Message::Text(
                json!({"type":"subscribe","channel":channel,"id":market})
                    .to_string()
                    .into(),
            ))
            .await?;
        }
    }
    let mut ping = interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping.tick() => {
                sink.send(Message::Ping(Vec::new().into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "dydx", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("dydx stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_dydx_events(&value) {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("dydx closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_dydx_markets_poll(markets: &[String], ctx: SourceContext) -> Result<()> {
    let client = reqwest::Client::new();
    let mut poll = interval(Duration::from_secs(10));
    loop {
        poll.tick().await;
        let value = client
            .get("https://indexer.dydx.trade/v4/perpetualMarkets")
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let Some(perpetual_markets) = value.get("markets").and_then(Value::as_object) else {
            continue;
        };
        for market in markets {
            let Some(row) = perpetual_markets.get(market) else {
                continue;
            };
            if let Some(open_interest) = string_f64(row, "openInterest") {
                ctx.emit(DataEvent::OpenInterest(OpenInterestTick {
                    exchange: "dydx",
                    symbol: market.clone().into_boxed_str(),
                    open_interest,
                    open_interest_value: None,
                    ts_ms: now_ms(),
                }))
                .await?;
            }
            if let Some(funding_rate) = string_f64(row, "nextFundingRate") {
                ctx.emit(DataEvent::FundingRate(FundingRateTick {
                    exchange: "dydx",
                    symbol: market.clone().into_boxed_str(),
                    funding_rate,
                    next_funding_time_ms: None,
                    mark_price: string_f64(row, "oraclePrice"),
                    index_price: string_f64(row, "oraclePrice"),
                    ts_ms: now_ms(),
                }))
                .await?;
            }
        }
    }
}

fn parse_dydx_events(value: &Value) -> Vec<DataEvent> {
    let channel = value
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let market = value
        .get("id")
        .or_else(|| value.get("market"))
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    let contents = value
        .get("contents")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    match channel {
        "v4_orderbook" => parse_orderbook(market, contents).into_iter().collect(),
        "v4_trades" => parse_trades(market, contents),
        _ => Vec::new(),
    }
}

fn parse_orderbook(market: &str, contents: &Value) -> Option<DataEvent> {
    let bids = contents
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_levels(items))
        .unwrap_or_default();
    let asks = contents
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_levels(items))
        .unwrap_or_default();
    Some(DataEvent::OrderBook(OrderBookTick {
        exchange: "dydx",
        market: MarketKind::Perp,
        symbol: market.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms: now_ms(),
    }))
}

fn parse_levels(items: &[Value]) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            let price = item
                .get("price")
                .or_else(|| item.get("price"))
                .and_then(Value::as_str)?;
            let qty = item
                .get("size")
                .or_else(|| item.get("size"))
                .and_then(Value::as_str)?;
            Some(BookLevel {
                price: price.parse::<f64>().ok()?,
                qty: qty.parse::<f64>().ok()?,
            })
        })
        .collect()
}

fn parse_trades(market: &str, contents: &Value) -> Vec<DataEvent> {
    let items = contents
        .get("trades")
        .and_then(Value::as_array)
        .or_else(|| contents.as_array());
    items
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "dydx",
                market: MarketKind::Perp,
                symbol: market.to_string().into_boxed_str(),
                price: string_f64(item, "price")?,
                qty: string_f64(item, "size")?,
                side: side_from_str(item.get("side").and_then(Value::as_str).unwrap_or_default()),
                trade_id: item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|x| x.to_string().into_boxed_str()),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn string_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_str()?.parse::<f64>().ok()
}

fn side_from_str(side: &str) -> TradeSide {
    match side {
        "BUY" | "buy" => TradeSide::Buy,
        "SELL" | "sell" => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn dydx_side_parser_accepts_api_labels() {
        assert_eq!(side_from_str("BUY"), TradeSide::Buy);
        assert_eq!(side_from_str("SELL"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }
}
