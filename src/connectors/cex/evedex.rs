use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{
    parse_exchange_datetime_ms, parse_object_levels, parse_value_f64, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const EVEDEX_REST_URL: &str = "https://exchange-api.evedex.com";

pub struct EvedexPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl EvedexPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for EvedexPerpFeed {
    fn name(&self) -> &'static str {
        "evedex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("evedex perp symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_evedex_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "evedex", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "evedex",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_evedex_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();
    let book = client
        .get(format!("{EVEDEX_REST_URL}/api/market/{symbol}/deep"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_evedex_book(symbol, &book));

    let trades = client
        .get(format!(
            "{EVEDEX_REST_URL}/api/market/{symbol}/recent-trades"
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_evedex_trades(symbol, &trades));

    let instrument = client
        .get(format!("{EVEDEX_REST_URL}/api/market/instrument"))
        .query(&[("instrument", symbol), ("fields", "metrics")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_evedex_funding(symbol, &instrument));
    events.extend(parse_evedex_open_interest(symbol, &instrument));

    Ok(events)
}

fn parse_evedex_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let ts_ms = value
        .get("t")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "quantity"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "quantity"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "evedex",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "evedex",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: Some(ts_ms),
        ts_ms,
    }));
    events
}

fn parse_evedex_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "evedex",
                market: MarketKind::Perp,
                symbol: symbol.to_string().into_boxed_str(),
                price: trade.get("fillPrice").and_then(parse_value_f64)?,
                qty: trade.get("fillQuantity").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("executionId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .and_then(parse_exchange_datetime_ms)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_evedex_funding(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(value);
    let Some(rate) = row.get("fundingRate").and_then(parse_value_f64) else {
        return Vec::new();
    };
    vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "evedex",
        symbol: symbol.to_string().into_boxed_str(),
        funding_rate: rate,
        next_funding_time_ms: Some(next_hour_ms(now_ms())),
        mark_price: row.get("markPrice").and_then(parse_value_f64),
        index_price: row
            .get("indexPrice")
            .or_else(|| row.pointer("/from/avgLastPrice"))
            .and_then(parse_value_f64),
        ts_ms: now_ms(),
    })]
}

fn parse_evedex_open_interest(symbol: &str, value: &Value) -> Vec<DataEvent> {
    instrument_rows(value)
        .into_iter()
        .filter_map(|row| {
            let open_interest = row.get("openInterest").and_then(parse_value_f64)?;
            let mark_price = row.get("markPrice").and_then(parse_value_f64);
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "evedex",
                symbol: row
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_string()
                    .into_boxed_str(),
                open_interest,
                open_interest_value: mark_price.map(|mark_price| mark_price * open_interest),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn instrument_rows(value: &Value) -> Vec<&Value> {
    value
        .as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_else(|| vec![value])
}

fn next_hour_ms(ts_ms: u64) -> u64 {
    ((ts_ms / 3_600_000) + 1) * 3_600_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evedex_parses_book_as_quote_and_book() {
        let events = parse_evedex_book(
            "BTCUSD",
            &json!({
                "t": 1779292730260_u64,
                "bids": [{"price": 77399.1, "quantity": 0.2}],
                "asks": [{"price": 77399.2, "quantity": 0.4}]
            }),
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn evedex_parses_trades() {
        let events = parse_evedex_trades(
            "BTCUSD",
            &json!([{
                "executionId": "267445437",
                "side": "BUY",
                "fillQuantity": 0.025,
                "fillPrice": 77413.6,
                "createdAt": "2026-05-21T03:37:46.376Z"
            }]),
        );
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.trade_id.as_deref(), Some("267445437"));
                assert_eq!(trade.ts_ms, 1779334666376);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn evedex_parses_funding() {
        let payload = json!([{
            "name": "BTCUSD",
            "markPrice": 77408.9,
            "fundingRate": 0.0001,
            "openInterest": 122.407
        }]);
        let funding = parse_evedex_funding("BTCUSD", &payload);
        let open_interest = parse_evedex_open_interest("BTCUSD", &payload);

        match &funding[0] {
            DataEvent::FundingRate(funding) => assert_eq!(funding.funding_rate, 0.0001),
            other => panic!("unexpected event: {other:?}"),
        }
        match &open_interest[0] {
            DataEvent::OpenInterest(oi) => {
                assert_eq!(oi.symbol.as_ref(), "BTCUSD");
                assert_eq!(oi.open_interest, 122.407);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn evedex_ignores_missing_open_interest() {
        let events = parse_evedex_open_interest(
            "BTCUSD",
            &json!([{
                "name": "BTCUSD",
                "markPrice": 77408.9,
                "fundingRate": 0.0001
            }]),
        );
        assert!(events.is_empty());
    }
}
