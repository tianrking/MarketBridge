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

const DERIBIT_REST_URL: &str = "https://www.deribit.com/api/v2/public";

pub struct DeribitPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl DeribitPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for DeribitPerpFeed {
    fn name(&self) -> &'static str {
        "deribit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("deribit perp symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_deribit_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "deribit", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "deribit",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_deribit_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{DERIBIT_REST_URL}/ticker"))
        .query(&[("instrument_name", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_deribit_ticker(symbol, &ticker));

    let book = client
        .get(format!("{DERIBIT_REST_URL}/get_order_book"))
        .query(&[("instrument_name", symbol), ("depth", "15")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_deribit_book(symbol, &book));

    let trades = client
        .get(format!("{DERIBIT_REST_URL}/get_last_trades_by_instrument"))
        .query(&[("instrument_name", symbol), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_deribit_trades(symbol, &trades));

    Ok(events)
}

fn result_row(value: &Value) -> &Value {
    value.get("result").unwrap_or(value)
}

fn parse_deribit_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = result_row(value);
    let symbol = row
        .get("instrument_name")
        .and_then(Value::as_str)
        .unwrap_or(symbol)
        .to_ascii_uppercase();
    let ts_ms = row
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::new();

    if let (Some(bid), Some(ask)) = (
        row.get("best_bid_price").and_then(parse_value_f64),
        row.get("best_ask_price").and_then(parse_value_f64),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "deribit",
            market: MarketKind::Perp,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: row.get("mark_price").and_then(parse_value_f64),
            funding_rate: row
                .get("funding_8h")
                .and_then(parse_value_f64)
                .or_else(|| row.get("current_funding").and_then(parse_value_f64)),
            ts_ms,
        }));
    }

    if let Some(funding_rate) = row
        .get("funding_8h")
        .and_then(parse_value_f64)
        .or_else(|| row.get("current_funding").and_then(parse_value_f64))
    {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "deribit",
            symbol: symbol.clone().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: None,
            mark_price: row.get("mark_price").and_then(parse_value_f64),
            index_price: row.get("index_price").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = row.get("open_interest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "deribit",
            symbol: symbol.into_boxed_str(),
            open_interest,
            open_interest_value: None,
            ts_ms,
        }));
    }

    events
}

fn parse_deribit_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = result_row(value);
    let symbol = row
        .get("instrument_name")
        .and_then(Value::as_str)
        .unwrap_or(symbol)
        .to_ascii_uppercase();
    let ts_ms = row
        .get("timestamp")
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
            exchange: "deribit",
            market: MarketKind::Perp,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: row.get("mark_price").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "deribit",
        market: MarketKind::Perp,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: row.get("change_id").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_deribit_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .pointer("/result/trades")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "deribit",
                market: MarketKind::Perp,
                symbol: trade
                    .get("instrument_name")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("direction")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("trade_id")
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
    fn parses_deribit_perp_market_data() {
        let ticker = parse_deribit_ticker(
            "BTC-PERPETUAL",
            &json!({"result": {
                "timestamp": 1779297516973_u64,
                "instrument_name": "BTC-PERPETUAL",
                "best_bid_price": 77362.0,
                "best_ask_price": 77362.5,
                "mark_price": 77368.0,
                "index_price": 77348.83,
                "funding_8h": 0.00000804,
                "open_interest": 1009051280.0
            }}),
        );
        let book = parse_deribit_book(
            "BTC-PERPETUAL",
            &json!({"result": {
                "timestamp": 1779297517062_u64,
                "change_id": 156019977874_u64,
                "instrument_name": "BTC-PERPETUAL",
                "bids": [[77362.0, 282620.0]],
                "asks": [[77362.5, 10010.0]]
            }}),
        );
        let trades = parse_deribit_trades(
            "BTC-PERPETUAL",
            &json!({"result": {"trades": [{
                "timestamp": 1779297526310_u64,
                "price": 77379.5,
                "amount": 10.0,
                "direction": "buy",
                "instrument_name": "BTC-PERPETUAL",
                "trade_id": "430104894"
            }]}}),
        );

        assert!(
            ticker
                .iter()
                .any(|event| matches!(event, DataEvent::Tick(_)))
        );
        assert!(
            ticker
                .iter()
                .any(|event| matches!(event, DataEvent::FundingRate(_)))
        );
        assert!(
            ticker
                .iter()
                .any(|event| matches!(event, DataEvent::OpenInterest(_)))
        );
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
    }
}
