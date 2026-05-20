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
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const BITHUMB_REST_URL: &str = "https://api.bithumb.com/public";

pub struct BithumbSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BithumbSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BithumbSpotFeed {
    fn name(&self) -> &'static str {
        "bithumb"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bithumb spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bithumb_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bithumb", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bithumb",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bithumb_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BITHUMB_REST_URL}/ticker/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bithumb_ticker(symbol, &ticker));

    let book = client
        .get(format!("{BITHUMB_REST_URL}/orderbook/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bithumb_book(symbol, &book));

    let trades = client
        .get(format!("{BITHUMB_REST_URL}/transaction_history/{symbol}"))
        .query(&[("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bithumb_trades(symbol, &trades));

    Ok(events)
}

fn parse_bithumb_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
    let bid = row
        .get("buy_price")
        .and_then(parse_value_f64)
        .or_else(|| row.get("closing_price").and_then(parse_value_f64));
    let ask = row
        .get("sell_price")
        .and_then(parse_value_f64)
        .or_else(|| row.get("closing_price").and_then(parse_value_f64));

    match (bid, ask) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "bithumb",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: row.get("closing_price").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: row
                .get("date")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_bithumb_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
    let ts_ms = row
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = row
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "quantity"))
        .unwrap_or_default();
    let asks = row
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
            exchange: "bithumb",
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
        exchange: "bithumb",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_bithumb_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter_map(|(idx, trade)| {
            let ts_ms = trade
                .get("transaction_date")
                .and_then(Value::as_str)
                .and_then(parse_exchange_datetime_ms)
                .unwrap_or_else(now_ms);
            Some(DataEvent::Trade(TradeTick {
                exchange: "bithumb",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("units_traded").and_then(parse_value_f64)?,
                side: trade
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["bid", "buy"], &["ask", "sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: Some(format!("{ts_ms}-{idx}").into_boxed_str()),
                ts_ms,
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_bithumb_book_and_trades() {
        let book_events = parse_bithumb_book(
            "BTC_KRW",
            &json!({
                "data": {
                    "timestamp": "1779295685614",
                    "bids": [{"price": "114786000", "quantity": "0.0103"}],
                    "asks": [{"price": "114792000", "quantity": "0.0586"}]
                }
            }),
        );
        let trade_events = parse_bithumb_trades(
            "BTC_KRW",
            &json!({
                "data": [{
                    "transaction_date": "2026-05-21 01:45:46",
                    "type": "bid",
                    "units_traded": "0.00015122",
                    "price": "114802000"
                }]
            }),
        );

        let DataEvent::OrderBook(book) = &book_events[1] else {
            panic!("expected order book");
        };
        assert_eq!(book.bids[0].price, 114786000.0);
        assert_eq!(book.asks[0].qty, 0.0586);

        let DataEvent::Trade(trade) = &trade_events[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.price, 114802000.0);
        assert_eq!(trade.ts_ms, 1779327946000);
    }

    #[test]
    fn parses_bithumb_ticker() {
        let events = parse_bithumb_ticker(
            "BTC_KRW",
            &json!({
                "data": {
                    "closing_price": "114786000",
                    "date": "1779295685791"
                }
            }),
        );

        let DataEvent::Tick(tick) = &events[0] else {
            panic!("expected tick");
        };
        assert_eq!(tick.bid, 114786000.0);
        assert_eq!(tick.ask, 114786000.0);
    }
}
