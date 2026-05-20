use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{
    parse_array_levels, parse_exchange_datetime_ms, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const COINCHECK_REST_URL: &str = "https://coincheck.com/api";

pub struct CoincheckSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl CoincheckSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoincheckSpotFeed {
    fn name(&self) -> &'static str {
        "coincheck"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("coincheck spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_coincheck_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "coincheck", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "coincheck",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_coincheck_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{COINCHECK_REST_URL}/ticker"))
        .query(&[("pair", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coincheck_ticker(symbol, &ticker));

    let book = client
        .get(format!("{COINCHECK_REST_URL}/order_books"))
        .query(&[("pair", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coincheck_book(symbol, &book));

    let trades = client
        .get(format!("{COINCHECK_REST_URL}/trades"))
        .query(&[("pair", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coincheck_trades(symbol, &trades));

    Ok(events)
}

fn parse_coincheck_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    match (
        value.get("bid").and_then(parse_value_f64),
        value.get("ask").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "coincheck",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_lowercase().into_boxed_str(),
            bid,
            ask,
            mark: value.get("last").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: value
                .get("timestamp")
                .and_then(parse_value_f64)
                .map(|secs| (secs * 1000.0) as u64)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_coincheck_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
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
            exchange: "coincheck",
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
        exchange: "coincheck",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_lowercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_coincheck_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "coincheck",
                market: MarketKind::Spot,
                symbol: trade
                    .get("pair")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_lowercase()
                    .into_boxed_str(),
                price: trade.get("rate").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("order_type")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("id")
                    .and_then(|value| {
                        value
                            .as_i64()
                            .map(|id| id.to_string())
                            .or_else(|| value.as_str().map(str::to_string))
                    })
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("created_at")
                    .and_then(Value::as_str)
                    .and_then(parse_exchange_datetime_ms)
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
    fn parses_coincheck_ticker_book_and_trade() {
        let ticker = parse_coincheck_ticker(
            "btc_jpy",
            &json!({
                "last": 12279825.0,
                "bid": 12277066.0,
                "ask": 12279391.0,
                "timestamp": 1779296340_u64
            }),
        );
        let book = parse_coincheck_book(
            "btc_jpy",
            &json!({
                "asks": [["12279146.0", "0.01"]],
                "bids": [["12277066.0", "0.02"]]
            }),
        );
        let trades = parse_coincheck_trades(
            "btc_jpy",
            &json!({
                "data": [{
                    "id": 304796175_i64,
                    "amount": "0.02",
                    "rate": "12279825.0",
                    "pair": "btc_jpy",
                    "order_type": "sell",
                    "created_at": "2026-05-20T16:58:43.000Z"
                }]
            }),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.ts_ms, 1779296323000);
        assert_eq!(trade.price, 12279825.0);
    }
}
