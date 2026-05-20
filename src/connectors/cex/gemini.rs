use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const GEMINI_REST_URL: &str = "https://api.gemini.com";

pub struct GeminiSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl GeminiSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for GeminiSpotFeed {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("gemini spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_gemini_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "gemini", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "gemini",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_gemini_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();
    let path_symbol = symbol.to_ascii_lowercase();

    let ticker = client
        .get(format!("{GEMINI_REST_URL}/v1/pubticker/{path_symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gemini_ticker(symbol, &ticker));

    let book = client
        .get(format!("{GEMINI_REST_URL}/v1/book/{path_symbol}"))
        .query(&[("limit_bids", "50"), ("limit_asks", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gemini_book(symbol, &book));

    let trades = client
        .get(format!("{GEMINI_REST_URL}/v1/trades/{path_symbol}"))
        .query(&[("limit_trades", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gemini_trades(symbol, &trades));

    Ok(events)
}

fn parse_gemini_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let Some(bid) = value.get("bid").and_then(parse_value_f64) else {
        return Vec::new();
    };
    let Some(ask) = value.get("ask").and_then(parse_value_f64) else {
        return Vec::new();
    };
    let ts_ms = value
        .pointer("/volume/timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);

    vec![DataEvent::Tick(MarketTick {
        exchange: "gemini",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bid,
        ask,
        mark: value.get("last").and_then(parse_value_f64),
        funding_rate: None,
        ts_ms,
    })]
}

fn parse_gemini_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "amount"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "amount"))
        .unwrap_or_default();
    let ts_ms = value
        .get("bids")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|row| row.get("timestamp"))
        .and_then(parse_value_f64)
        .map(|secs| (secs * 1000.0) as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "gemini",
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
        exchange: "gemini",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));
    events
}

fn parse_gemini_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "gemini",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("tid")
                    .and_then(|value| value.as_i64().map(|x| x.to_string()))
                    .or_else(|| {
                        trade
                            .get("tid")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("timestampms")
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
    fn gemini_parses_ticker() {
        let events = parse_gemini_ticker(
            "BTCUSD",
            &json!({
                "bid": "9117.95",
                "ask": "9117.96",
                "last": "9115.23",
                "volume": {"timestamp": 1594982700000_u64}
            }),
        );
        assert!(matches!(events[0], DataEvent::Tick(_)));
    }

    #[test]
    fn gemini_parses_book_as_quote_and_book() {
        let events = parse_gemini_book(
            "BTCUSD",
            &json!({
                "bids": [{"price": "9117.95", "amount": "0.1", "timestamp": "1601617445"}],
                "asks": [{"price": "9117.96", "amount": "0.2", "timestamp": "1601617445"}]
            }),
        );
        assert_eq!(events.len(), 2);
        match &events[1] {
            DataEvent::OrderBook(book) => assert_eq!(book.asks[0].qty, 0.2),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn gemini_parses_trades() {
        let events = parse_gemini_trades(
            "BTCUSD",
            &json!([{
                "timestamp": 1601617445,
                "timestampms": 1601617445144_u64,
                "tid": 14122489752_i64,
                "price": "0.46476",
                "amount": "28.407209",
                "type": "buy"
            }]),
        );
        match &events[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.trade_id.as_deref(), Some("14122489752")),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
