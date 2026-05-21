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

const BITFLYER_REST_URL: &str = "https://api.bitflyer.com/v1";

pub struct BitflyerSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitflyerSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitflyerSpotFeed {
    fn name(&self) -> &'static str {
        "bitflyer"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitflyer spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitflyer_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "bitflyer", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "bitflyer",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_bitflyer_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{BITFLYER_REST_URL}/getticker"))
        .query(&[("product_code", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitflyer_ticker(symbol, &ticker));

    let book = client
        .get(format!("{BITFLYER_REST_URL}/getboard"))
        .query(&[("product_code", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitflyer_book(symbol, &book));

    let trades = client
        .get(format!("{BITFLYER_REST_URL}/getexecutions"))
        .query(&[("product_code", symbol), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitflyer_trades(symbol, &trades));

    Ok(events)
}

fn parse_bitflyer_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    match (
        value.get("best_bid").and_then(parse_value_f64),
        value.get("best_ask").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "bitflyer",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: value.get("ltp").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_exchange_datetime_ms)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_bitflyer_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "size"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "size"))
        .unwrap_or_default();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitflyer",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: value.get("mid_price").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "bitflyer",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_bitflyer_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitflyer",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("size").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
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
                    .get("exec_date")
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
    fn parses_bitflyer_public_market_data() {
        let ticker = parse_bitflyer_ticker(
            "BTC_JPY",
            &json!({
                "timestamp": "2026-05-20T17:01:19.933",
                "best_bid": 12269508.0,
                "best_ask": 12276609.0,
                "ltp": 12276609.0
            }),
        );
        let book = parse_bitflyer_book(
            "BTC_JPY",
            &json!({
                "mid_price": 12273058.0,
                "bids": [{"price": 12269508.0, "size": 0.0019994}],
                "asks": [{"price": 12276609.0, "size": 0.023}]
            }),
        );
        let trades = parse_bitflyer_trades(
            "BTC_JPY",
            &json!([{
                "id": 2644824470_i64,
                "side": "BUY",
                "price": 12276609.0,
                "size": 0.04,
                "exec_date": "2026-05-20T17:01:19.783"
            }]),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.trade_id.as_deref(), Some("2644824470"));
        assert_eq!(trade.ts_ms, 1779296479783);
    }
}
