use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::parse_value_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const NDAX_REST_URL: &str = "https://api.ndax.io:8443/AP";

#[derive(Debug, Clone)]
struct NdaxMarket {
    instrument_id: u64,
    symbol: String,
}

pub struct NdaxSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl NdaxSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for NdaxSpotFeed {
    fn name(&self) -> &'static str {
        "ndax"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        let markets = resolve_ndax_markets(&self.client, &self.symbols).await?;
        if markets.is_empty() {
            anyhow::bail!("ndax spot markets empty");
        }

        let mut last_trade_ids = HashMap::<String, u64>::new();
        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for market in &markets {
                match poll_ndax_book(&self.client, market).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "ndax", symbol = %market.symbol, error = %err, "poll failed")
                    }
                }
                match poll_ndax_trades(
                    &self.client,
                    market,
                    last_trade_ids.get(&market.symbol).copied(),
                )
                .await
                {
                    Ok((events, max_trade_id)) => {
                        if let Some(trade_id) = max_trade_id {
                            last_trade_ids.insert(market.symbol.clone(), trade_id);
                        }
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "ndax", symbol = %market.symbol, error = %err, "trade poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "ndax",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn resolve_ndax_markets(
    client: &reqwest::Client,
    symbols: &[String],
) -> Result<Vec<NdaxMarket>> {
    let value = client
        .get(format!("{NDAX_REST_URL}/GetInstruments"))
        .query(&[("OMSId", "1")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let rows = value.as_array().context("ndax instruments missing")?;
    let index = rows
        .iter()
        .filter(|row| row.get("IsDisable").and_then(Value::as_bool) != Some(true))
        .filter_map(parse_ndax_market)
        .map(|market| (compact_symbol(&market.symbol), market))
        .collect::<HashMap<_, _>>();
    Ok(symbols
        .iter()
        .filter_map(|symbol| index.get(&compact_symbol(symbol)).cloned())
        .collect())
}

async fn poll_ndax_book(client: &reqwest::Client, market: &NdaxMarket) -> Result<Vec<DataEvent>> {
    let instrument_id = market.instrument_id.to_string();
    let value = client
        .get(format!("{NDAX_REST_URL}/GetL2Snapshot"))
        .query(&[
            ("OMSId", "1"),
            ("InstrumentId", instrument_id.as_str()),
            ("Depth", "200"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_ndax_book(market, &value))
}

async fn poll_ndax_trades(
    client: &reqwest::Client,
    market: &NdaxMarket,
    last_seen_trade: Option<u64>,
) -> Result<(Vec<DataEvent>, Option<u64>)> {
    let instrument_id = market.instrument_id.to_string();
    let value = client
        .get(format!("{NDAX_REST_URL}/GetLastTrades"))
        .query(&[
            ("OMSId", "1"),
            ("InstrumentId", instrument_id.as_str()),
            ("Count", "100"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_ndax_trades(market, &value, last_seen_trade))
}

fn parse_ndax_market(row: &Value) -> Option<NdaxMarket> {
    Some(NdaxMarket {
        instrument_id: row.get("InstrumentId").and_then(Value::as_u64)?,
        symbol: row
            .get("Symbol")
            .and_then(Value::as_str)?
            .to_ascii_uppercase(),
    })
}

fn parse_ndax_book(market: &NdaxMarket, value: &Value) -> Vec<DataEvent> {
    let rows = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let ts_ms = rows
        .first()
        .and_then(|row| row.as_array())
        .and_then(|row| row.get(2))
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    let mut last_update_id = None;
    for row in rows {
        let Some(fields) = row.as_array() else {
            continue;
        };
        last_update_id = fields.first().and_then(Value::as_u64).or(last_update_id);
        let Some(level) = ndax_level(fields) else {
            continue;
        };
        match fields.get(9).and_then(Value::as_u64) {
            Some(0) => bids.push(level),
            Some(1) => asks.push(level),
            _ => {}
        }
    }

    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "ndax",
            market: MarketKind::Spot,
            symbol: market.symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "ndax",
        market: MarketKind::Spot,
        symbol: market.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id,
        ts_ms,
    }));
    events
}

fn parse_ndax_trades(
    market: &NdaxMarket,
    value: &Value,
    last_seen_trade: Option<u64>,
) -> (Vec<DataEvent>, Option<u64>) {
    let rows = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut max_trade_id = last_seen_trade;
    let mut events = Vec::new();
    for item in rows {
        let Some(fields) = item.as_array() else {
            continue;
        };
        let trade_id = fields.first().and_then(Value::as_u64);
        if trade_id.is_some_and(|id| last_seen_trade.is_some_and(|last| id <= last)) {
            continue;
        }
        if let Some(id) = trade_id {
            max_trade_id = Some(max_trade_id.map_or(id, |max| max.max(id)));
        }
        let Some(qty) = fields.get(2).and_then(parse_value_f64) else {
            continue;
        };
        let Some(price) = fields.get(3).and_then(parse_value_f64) else {
            continue;
        };
        events.push(DataEvent::Trade(TradeTick {
            exchange: "ndax",
            market: MarketKind::Spot,
            symbol: market.symbol.clone().into_boxed_str(),
            price,
            qty,
            side: match fields.get(8).and_then(Value::as_u64) {
                Some(0) => TradeSide::Buy,
                Some(1) => TradeSide::Sell,
                _ => TradeSide::Unknown,
            },
            trade_id: trade_id.map(|id| id.to_string().into_boxed_str()),
            ts_ms: fields
                .get(6)
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms),
        }));
    }
    (events, max_trade_id)
}

fn ndax_level(fields: &[Value]) -> Option<BookLevel> {
    Some(BookLevel {
        price: fields.get(6).and_then(parse_value_f64)?,
        qty: fields.get(8).and_then(parse_value_f64)?,
    })
}

fn compact_symbol(symbol: &str) -> String {
    symbol
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ndax_parses_market_metadata() {
        let market = parse_ndax_market(&json!({
            "InstrumentId": 1,
            "Symbol": "BTCCAD",
            "IsDisable": false
        }))
        .expect("market");
        assert_eq!(market.instrument_id, 1);
        assert_eq!(market.symbol, "BTCCAD");
    }

    #[test]
    fn ndax_parses_book_as_quote_and_book() {
        let market = NdaxMarket {
            instrument_id: 1,
            symbol: "BTCCAD".to_string(),
        };
        let events = parse_ndax_book(
            &market,
            &json!([
                [
                    404416356,
                    1,
                    1779292580095_u64,
                    0,
                    106298.6,
                    1,
                    106296.78,
                    1,
                    0.0121195,
                    0
                ],
                [
                    404416356,
                    1,
                    1779292580095_u64,
                    0,
                    106298.6,
                    1,
                    106743.95,
                    1,
                    0.05940795,
                    1
                ]
            ]),
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        match &events[1] {
            DataEvent::OrderBook(book) => assert_eq!(book.last_update_id, Some(404416356)),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn ndax_parses_recent_trades_with_dedupe() {
        let market = NdaxMarket {
            instrument_id: 8,
            symbol: "BTCCAD".to_string(),
        };
        let (events, max_trade_id) = parse_ndax_trades(
            &market,
            &json!([
                [
                    6913253,
                    8,
                    0.03340802,
                    19116.08,
                    2543425077_u64,
                    2543425482_u64,
                    1606935922416_u64,
                    0,
                    1,
                    0,
                    0
                ],
                [
                    6913254,
                    8,
                    0.01391671,
                    19117.42,
                    2543427510_u64,
                    2543427811_u64,
                    1606935927998_u64,
                    1,
                    0,
                    0,
                    0
                ]
            ]),
            Some(6913253),
        );
        assert_eq!(max_trade_id, Some(6913254));
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.trade_id.as_deref(), Some("6913254"));
                assert_eq!(trade.side, TradeSide::Buy);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
