use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const BITVAVO_REST_URL: &str = "https://api.bitvavo.com/v2";

pub struct BitvavoSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitvavoSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitvavoSpotFeed {
    fn name(&self) -> &'static str {
        "bitvavo"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitvavo spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitvavo_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bitvavo", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bitvavo",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bitvavo_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BITVAVO_REST_URL}/ticker/book"))
        .query(&[("market", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitvavo_ticker(symbol, &ticker));

    let book = client
        .get(format!("{BITVAVO_REST_URL}/{symbol}/book"))
        .query(&[("depth", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitvavo_book(symbol, &book));

    let trades = client
        .get(format!("{BITVAVO_REST_URL}/{symbol}/trades"))
        .query(&[("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitvavo_trades(symbol, &trades));

    Ok(events)
}

fn parse_bitvavo_ticker(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let symbol = value
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol);
    match (
        value.get("bid").and_then(parse_value_f64),
        value.get("ask").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "bitvavo",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms: now_ms(),
        })],
        _ => Vec::new(),
    }
}

fn parse_bitvavo_book(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let symbol = value
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
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
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitvavo",
            market: MarketKind::Spot,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitvavo",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("nonce").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_bitvavo_trades(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitvavo",
                market: MarketKind::Spot,
                symbol: fallback_symbol.to_ascii_uppercase().into_boxed_str(),
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
                    .get("timestamp")
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
    fn parses_bitvavo_book_and_trade() {
        let book_events = parse_bitvavo_book(
            "BTC-EUR",
            &json!({
                "market": "BTC-EUR",
                "nonce": 306090870_u64,
                "bids": [["66453", "0.00075237"]],
                "asks": [["66454", "0.18648815"]]
            }),
        );
        let trades = parse_bitvavo_trades(
            "BTC-EUR",
            &json!([{
                "id": "00000000-0000-0431-0000-000003427476",
                "timestamp": 1779295813006_u64,
                "amount": "0.0002251",
                "price": "66457",
                "side": "buy"
            }]),
        );

        let DataEvent::OrderBook(book) = &book_events[1] else {
            panic!("expected order book");
        };
        assert_eq!(book.last_update_id, Some(306090870));
        assert_eq!(book.asks[0].price, 66454.0);

        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.price, 66457.0);
        assert_eq!(trade.ts_ms, 1779295813006);
    }
}
