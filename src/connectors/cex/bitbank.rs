use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const BITBANK_REST_URL: &str = "https://public.bitbank.cc";

pub struct BitbankSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitbankSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitbankSpotFeed {
    fn name(&self) -> &'static str {
        "bitbank"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitbank spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitbank_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bitbank", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bitbank",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bitbank_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BITBANK_REST_URL}/{symbol}/ticker"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitbank_ticker(symbol, &ticker));

    let book = client
        .get(format!("{BITBANK_REST_URL}/{symbol}/depth"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitbank_book(symbol, &book));

    let trades = client
        .get(format!("{BITBANK_REST_URL}/{symbol}/transactions"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitbank_trades(symbol, &trades));

    Ok(events)
}

fn parse_bitbank_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
    match (
        row.get("buy").and_then(parse_value_f64),
        row.get("sell").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "bitbank",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_lowercase().into_boxed_str(),
            bid,
            ask,
            mark: row.get("last").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: row
                .get("timestamp")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_bitbank_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
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
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitbank",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_lowercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitbank",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_lowercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_bitbank_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .pointer("/data/transactions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitbank",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_lowercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("transaction_id")
                    .and_then(|value| {
                        value
                            .as_i64()
                            .map(|id| id.to_string())
                            .or_else(|| value.as_str().map(str::to_string))
                    })
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("executed_at")
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
    fn parses_bitbank_public_market_data() {
        let ticker = parse_bitbank_ticker(
            "btc_jpy",
            &json!({
                "data": {
                    "sell": "12280757",
                    "buy": "12280756",
                    "last": "12277156",
                    "timestamp": 1779296609094_u64
                }
            }),
        );
        let book = parse_bitbank_book(
            "btc_jpy",
            &json!({
                "data": {
                    "asks": [["12280757", "2.2665"]],
                    "bids": [["12280756", "0.0032"]]
                }
            }),
        );
        let trades = parse_bitbank_trades(
            "btc_jpy",
            &json!({
                "data": {
                    "transactions": [{
                        "transaction_id": 1230562317_i64,
                        "side": "sell",
                        "price": "12277156",
                        "amount": "0.0032",
                        "executed_at": 1779296467166_u64
                    }]
                }
            }),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.trade_id.as_deref(), Some("1230562317"));
        assert_eq!(trade.ts_ms, 1779296467166);
    }
}
