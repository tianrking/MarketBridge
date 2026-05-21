use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const BLOFIN_REST_URL: &str = "https://openapi.blofin.com/api/v1";

pub struct BlofinPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BlofinPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BlofinPerpFeed {
    fn name(&self) -> &'static str {
        "blofin"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("blofin perp symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_blofin_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "blofin", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "blofin",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_blofin_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BLOFIN_REST_URL}/market/tickers"))
        .query(&[("instId", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_blofin_ticker(symbol, &ticker));

    let book = client
        .get(format!("{BLOFIN_REST_URL}/market/books"))
        .query(&[("instId", symbol), ("size", "15")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_blofin_book(symbol, &book));

    let trades = client
        .get(format!("{BLOFIN_REST_URL}/market/trades"))
        .query(&[("instId", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_blofin_trades(symbol, &trades));

    let funding = client
        .get(format!("{BLOFIN_REST_URL}/market/funding-rate"))
        .query(&[("instId", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_blofin_funding(symbol, &funding));

    let open_interest = client
        .get(format!("{BLOFIN_REST_URL}/market/open-interest"))
        .query(&[("instId", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_blofin_open_interest(symbol, &open_interest));

    Ok(events)
}

fn first_data_row(value: &Value) -> &Value {
    value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .unwrap_or(value)
}

fn parse_blofin_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = first_data_row(value);
    match (
        row.get("bidPrice").and_then(parse_value_f64),
        row.get("askPrice").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "blofin",
            market: MarketKind::Perp,
            symbol: row
                .get("instId")
                .and_then(Value::as_str)
                .unwrap_or(symbol)
                .to_ascii_uppercase()
                .into_boxed_str(),
            bid,
            ask,
            mark: row.get("last").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: row
                .get("ts")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_blofin_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = first_data_row(value);
    let ts_ms = row
        .get("ts")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
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
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "blofin",
            market: MarketKind::Perp,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "blofin",
        market: MarketKind::Perp,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_blofin_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "blofin",
                market: MarketKind::Perp,
                symbol: trade
                    .get("instId")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("size").and_then(parse_value_f64)?,
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
                    .get("ts")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_blofin_funding(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = first_data_row(value);
    row.get("fundingRate")
        .and_then(parse_value_f64)
        .map(|funding_rate| {
            vec![DataEvent::FundingRate(FundingRateTick {
                exchange: "blofin",
                symbol: row
                    .get("instId")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                funding_rate,
                next_funding_time_ms: row
                    .get("fundingTime")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64),
                mark_price: None,
                index_price: None,
                ts_ms: now_ms(),
            })]
        })
        .unwrap_or_default()
}

fn parse_blofin_open_interest(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = first_data_row(value);
    row.get("openInterestCurrency")
        .or_else(|| row.get("openInterest"))
        .and_then(parse_value_f64)
        .map(|open_interest| {
            vec![DataEvent::OpenInterest(OpenInterestTick {
                exchange: "blofin",
                symbol: row
                    .get("instId")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                open_interest,
                open_interest_value: row.get("openInterest").and_then(parse_value_f64),
                ts_ms: row
                    .get("ts")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            })]
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_blofin_perp_market_data() {
        let ticker = parse_blofin_ticker(
            "BTC-USDT",
            &json!({"data": [{
                "instId": "BTC-USDT",
                "last": "77380.7",
                "askPrice": "77384.3",
                "bidPrice": "77384.2",
                "ts": "1779297249837"
            }]}),
        );
        let book = parse_blofin_book(
            "BTC-USDT",
            &json!({"data": [{
                "asks": [["77388.2", "720"]],
                "bids": [["77388", "14"]],
                "ts": "1779297259980"
            }]}),
        );
        let trades = parse_blofin_trades(
            "BTC-USDT",
            &json!({"data": [{
                "tradeId": "46114734870",
                "instId": "BTC-USDT",
                "price": "77388.2",
                "size": "4",
                "side": "buy",
                "ts": "1779297257882"
            }]}),
        );
        let funding = parse_blofin_funding(
            "BTC-USDT",
            &json!({"data": [{
                "instId": "BTC-USDT",
                "fundingRate": "0.00003929",
                "fundingTime": "1779321600000"
            }]}),
        );
        let open_interest = parse_blofin_open_interest(
            "BTC-USDT",
            &json!({"data": [{
                "instId": "BTC-USDT",
                "openInterest": "5284413",
                "openInterestCurrency": "5284.413",
                "ts": "1779330660000"
            }]}),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
        assert!(matches!(funding[0], DataEvent::FundingRate(_)));
        assert!(matches!(open_interest[0], DataEvent::OpenInterest(_)));
    }
}
