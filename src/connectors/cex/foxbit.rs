use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const FOXBIT_REST_URL: &str = "https://api.foxbit.com.br/rest/v3";

pub struct FoxbitSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl FoxbitSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for FoxbitSpotFeed {
    fn name(&self) -> &'static str {
        "foxbit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("foxbit spot symbols empty");
        }
        let mut last_trade_ids = HashMap::<String, u64>::new();
        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_foxbit_book(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "foxbit", symbol, error = %err, "poll failed"),
                }
                match poll_foxbit_trades(&self.client, symbol, last_trade_ids.get(symbol).copied())
                    .await
                {
                    Ok((events, max_trade_id)) => {
                        if let Some(trade_id) = max_trade_id {
                            last_trade_ids.insert(symbol.clone(), trade_id);
                        }
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "foxbit", symbol, error = %err, "trade poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "foxbit",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_foxbit_book(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let value = client
        .get(format!(
            "{FOXBIT_REST_URL}/markets/{}/orderbook",
            symbol.to_ascii_lowercase()
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_foxbit_book(symbol, &value))
}

async fn poll_foxbit_trades(
    client: &reqwest::Client,
    symbol: &str,
    last_seen_trade: Option<u64>,
) -> Result<(Vec<DataEvent>, Option<u64>)> {
    let value = client
        .get(format!(
            "{FOXBIT_REST_URL}/markets/{}/trades/history",
            symbol.to_ascii_lowercase()
        ))
        .query(&[("page_size", "100")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_foxbit_trades(symbol, &value, last_seen_trade))
}

fn parse_foxbit_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let ts_ms = value
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
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
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "foxbit",
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
        exchange: "foxbit",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("sequence_id").and_then(Value::as_u64),
        ts_ms,
    }));
    events
}

fn parse_foxbit_trades(
    symbol: &str,
    value: &Value,
    last_seen_trade: Option<u64>,
) -> (Vec<DataEvent>, Option<u64>) {
    let rows = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut max_trade_id = last_seen_trade;
    let mut events = Vec::new();
    for item in rows {
        let trade_id = item.get("id").and_then(parse_value_f64).map(|id| id as u64);
        if trade_id.is_some_and(|id| last_seen_trade.is_some_and(|last| id <= last)) {
            continue;
        }
        if let Some(id) = trade_id {
            max_trade_id = Some(max_trade_id.map_or(id, |max| max.max(id)));
        }
        let Some(price) = item.get("price").and_then(parse_value_f64) else {
            continue;
        };
        let Some(qty) = item
            .get("volume")
            .or_else(|| item.get("quantity"))
            .and_then(parse_value_f64)
        else {
            continue;
        };
        events.push(DataEvent::Trade(TradeTick {
            exchange: "foxbit",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            price,
            qty,
            side: match item
                .get("taker_side")
                .or_else(|| item.get("side"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "buy" => TradeSide::Buy,
                "sell" => TradeSide::Sell,
                _ => TradeSide::Unknown,
            },
            trade_id: trade_id.map(|id| id.to_string().into_boxed_str()),
            ts_ms: now_ms(),
        }));
    }
    (events, max_trade_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn foxbit_parses_book_as_quote_and_book() {
        let events = parse_foxbit_book(
            "btcbrl",
            &json!({
                "sequence_id": 23661649,
                "timestamp": 1779292421366_u64,
                "bids": [["388500.0", "0.1"]],
                "asks": [["388603.0", "0.2"]]
            }),
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        match &events[1] {
            DataEvent::OrderBook(book) => {
                assert_eq!(book.symbol.as_ref(), "BTCBRL");
                assert_eq!(book.last_update_id, Some(23661649));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn foxbit_parses_recent_trades_with_dedupe() {
        let (events, max_trade_id) = parse_foxbit_trades(
            "btcbrl",
            &json!({
                "data": [
                    {"id": 1, "price": "329248.747", "volume": "0.001", "taker_side": "BUY", "created_at": "2024-01-01T00:00:00Z"},
                    {"id": 2, "price": "329249.000", "volume": "0.002", "taker_side": "SELL", "created_at": "2024-01-01T00:00:01Z"}
                ]
            }),
            Some(1),
        );
        assert_eq!(max_trade_id, Some(2));
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.trade_id.as_deref(), Some("2"));
                assert_eq!(trade.side, TradeSide::Sell);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
