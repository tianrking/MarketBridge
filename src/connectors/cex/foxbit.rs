use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, now_ms};

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
}
