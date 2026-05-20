use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick, now_ms,
};

const BULLISH_REST_URL: &str = "https://api.exchange.bullish.com/trading-api/v1";

pub struct BullishSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BullishSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BullishSpotFeed {
    fn name(&self) -> &'static str {
        "bullish"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bullish spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bullish_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bullish", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bullish",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bullish_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BULLISH_REST_URL}/markets/{symbol}/tick"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bullish_ticker(symbol, &ticker));

    let book = client
        .get(format!(
            "{BULLISH_REST_URL}/markets/{symbol}/orderbook/hybrid"
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bullish_book(symbol, &book));

    let trades = client
        .get(format!(
            "{BULLISH_REST_URL}/history/markets/{symbol}/trades"
        ))
        .query(&[("_pageSize", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bullish_trades(symbol, &trades));

    Ok(events)
}

fn parse_bullish_ticker(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let symbol = value
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
    let ts_ms = value
        .get("createdAtTimestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::new();

    if let (Some(bid), Some(ask)) = (
        value
            .get("bestBid")
            .or_else(|| value.get("bid"))
            .and_then(parse_value_f64),
        value
            .get("bestAsk")
            .or_else(|| value.get("ask"))
            .and_then(parse_value_f64),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bullish",
            market: MarketKind::Spot,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: value
                .get("markPrice")
                .and_then(parse_value_f64)
                .or_else(|| value.get("last").and_then(parse_value_f64)),
            funding_rate: value.get("fundingRate").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = value.get("openInterest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "bullish",
            symbol: symbol.into_boxed_str(),
            open_interest,
            open_interest_value: None,
            ts_ms,
        }));
    }

    events
}

fn parse_bullish_book(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let ts_ms = value
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "priceLevelQuantity"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "priceLevelQuantity"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bullish",
            market: MarketKind::Spot,
            symbol: fallback_symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bullish",
        market: MarketKind::Spot,
        symbol: fallback_symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("sequenceNumber").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_bullish_trades(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "bullish",
                market: MarketKind::Spot,
                symbol: trade
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback_symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("quantity").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("tradeId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("createdAtTimestamp")
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
    fn parses_bullish_public_market_data() {
        let ticker = parse_bullish_ticker(
            "BTCUSDC",
            &json!({
                "symbol": "BTCUSDC",
                "createdAtTimestamp": "1621490985000",
                "bestBid": "1.00000000",
                "bestAsk": "1.10000000",
                "last": "1.05000000",
                "openInterest": "100000.32452"
            }),
        );
        let book = parse_bullish_book(
            "BTCUSDC",
            &json!({
                "bids": [{"price": "1.00000000", "priceLevelQuantity": "2.00000000"}],
                "asks": [{"price": "1.10000000", "priceLevelQuantity": "3.00000000"}],
                "timestamp": "1621490985000",
                "sequenceNumber": 999_u64
            }),
        );
        let trades = parse_bullish_trades(
            "BTCUSDC",
            &json!([{
                "tradeId": "100178000000367159",
                "symbol": "BTCUSDC",
                "price": "103891.8977",
                "quantity": "0.00029411",
                "side": "BUY",
                "createdAtTimestamp": "1747768055826"
            }]),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(ticker[1], DataEvent::OpenInterest(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
    }
}
