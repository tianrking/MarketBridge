use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms,
};

const UPBIT_REST_URL: &str = "https://api.upbit.com/v1";

pub struct UpbitSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl UpbitSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for UpbitSpotFeed {
    fn name(&self) -> &'static str {
        "upbit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("upbit spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_upbit_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "upbit", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "upbit",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_upbit_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{UPBIT_REST_URL}/ticker"))
        .query(&[("markets", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_upbit_ticker(symbol, &ticker));

    let book = client
        .get(format!("{UPBIT_REST_URL}/orderbook"))
        .query(&[("markets", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_upbit_book(symbol, &book));

    let trades = client
        .get(format!("{UPBIT_REST_URL}/trades/ticks"))
        .query(&[("market", symbol), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_upbit_trades(symbol, &trades));

    Ok(events)
}

fn parse_upbit_ticker(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|ticker| {
            let symbol = ticker
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            let last = ticker.get("trade_price").and_then(parse_value_f64)?;
            Some(DataEvent::Tick(MarketTick {
                exchange: "upbit",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                bid: last,
                ask: last,
                mark: Some(last),
                funding_rate: None,
                ts_ms: ticker
                    .get("timestamp")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_upbit_book(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let Some(row) = value.as_array().and_then(|items| items.first()) else {
        return Vec::new();
    };
    let symbol = row
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
    let ts_ms = row
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let units = row
        .get("orderbook_units")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let bids = units
        .iter()
        .filter_map(|unit| {
            Some(BookLevel {
                price: unit.get("bid_price").and_then(parse_value_f64)?,
                qty: unit.get("bid_size").and_then(parse_value_f64)?,
            })
        })
        .collect::<Vec<_>>();
    let asks = units
        .iter()
        .filter_map(|unit| {
            Some(BookLevel {
                price: unit.get("ask_price").and_then(parse_value_f64)?,
                qty: unit.get("ask_size").and_then(parse_value_f64)?,
            })
        })
        .collect::<Vec<_>>();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "upbit",
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
        exchange: "upbit",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_upbit_trades(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            let symbol = trade
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Trade(TradeTick {
                exchange: "upbit",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("trade_price").and_then(parse_value_f64)?,
                qty: trade.get("trade_volume").and_then(parse_value_f64)?,
                side: trade
                    .get("ask_bid")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["bid"], &["ask"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("sequential_id")
                    .and_then(|value| {
                        value
                            .as_i64()
                            .map(|id| id.to_string())
                            .or_else(|| value.as_str().map(str::to_string))
                    })
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
    fn parses_upbit_book() {
        let events = parse_upbit_book(
            "USDT-BTC",
            &json!([{
                "market": "USDT-BTC",
                "timestamp": 1779295528159_u64,
                "orderbook_units": [{
                    "bid_price": 77054.5,
                    "bid_size": 0.00020885,
                    "ask_price": 77624.24,
                    "ask_size": 0.06153677
                }]
            }]),
        );

        assert!(matches!(events[0], DataEvent::Tick(_)));
        let DataEvent::OrderBook(book) = &events[1] else {
            panic!("expected order book");
        };
        assert_eq!(book.symbol.as_ref(), "USDT-BTC");
        assert_eq!(book.bids[0].price, 77054.5);
    }

    #[test]
    fn parses_upbit_trade() {
        let events = parse_upbit_trades(
            "USDT-BTC",
            &json!([{
                "market": "USDT-BTC",
                "timestamp": 1779294123943_u64,
                "trade_price": 77694.6,
                "trade_volume": 0.00836608,
                "ask_bid": "BID",
                "sequential_id": 17792941239430000_i64
            }]),
        );

        let DataEvent::Trade(trade) = &events[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.price, 77694.6);
        assert_eq!(trade.ts_ms, 1779294123943);
    }
}
