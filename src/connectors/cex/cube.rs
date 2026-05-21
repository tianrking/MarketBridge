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

const CUBE_REST_URL: &str = "https://api.cube.exchange";

#[derive(Debug, Clone)]
struct CubeMarket {
    market_id: u64,
    symbol: String,
    price_scaler: f64,
    quantity_scaler: f64,
}

pub struct CubeSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl CubeSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CubeSpotFeed {
    fn name(&self) -> &'static str {
        "cube"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        let markets = resolve_cube_markets(&self.client, &self.symbols).await?;
        if markets.is_empty() {
            anyhow::bail!("cube spot markets empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        let mut last_trade_ids = HashMap::<u64, u64>::new();
        loop {
            tick.tick().await;
            for market in &markets {
                let since_trade_id = last_trade_ids.get(&market.market_id).copied();
                match poll_cube_market(&self.client, market, since_trade_id).await {
                    Ok((events, max_trade_id)) => {
                        if let Some(max_trade_id) = max_trade_id {
                            last_trade_ids.insert(market.market_id, max_trade_id);
                        }
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "cube", symbol = %market.symbol, error = %err, "poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "cube",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn resolve_cube_markets(
    client: &reqwest::Client,
    symbols: &[String],
) -> Result<Vec<CubeMarket>> {
    let value = client
        .get(format!("{CUBE_REST_URL}/ir/v0/markets"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let rows = value
        .pointer("/result/markets")
        .and_then(Value::as_array)
        .context("cube markets missing")?;
    let index = rows
        .iter()
        .filter(|row| row.get("disabled").and_then(Value::as_bool) != Some(true))
        .filter_map(parse_cube_market)
        .map(|market| (compact_symbol(&market.symbol), market))
        .collect::<HashMap<_, _>>();

    Ok(symbols
        .iter()
        .filter_map(|symbol| index.get(&compact_symbol(symbol)).cloned())
        .collect())
}

async fn poll_cube_market(
    client: &reqwest::Client,
    market: &CubeMarket,
    since_trade_id: Option<u64>,
) -> Result<(Vec<DataEvent>, Option<u64>)> {
    let value = client
        .get(format!(
            "{CUBE_REST_URL}/md/v0/book/{}/snapshot",
            market.market_id
        ))
        .query(&[("mbp", "true"), ("levels", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let mut events = parse_cube_snapshot(market, &value);

    let trades = client
        .get(format!(
            "{CUBE_REST_URL}/md/v0/book/{}/recent-trades",
            market.market_id
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let (trade_events, max_trade_id) = parse_cube_trades(market, &trades, since_trade_id);
    events.extend(trade_events);

    Ok((events, max_trade_id))
}

fn parse_cube_market(row: &Value) -> Option<CubeMarket> {
    let status = row.get("status").and_then(Value::as_i64).unwrap_or(1);
    if !matches!(status, 1 | 2) {
        return None;
    }
    Some(CubeMarket {
        market_id: row.get("marketId").and_then(Value::as_u64)?,
        symbol: row
            .get("symbol")
            .and_then(Value::as_str)?
            .to_ascii_uppercase(),
        price_scaler: row
            .get("priceTickSize")
            .and_then(parse_value_f64)
            .unwrap_or(1.0),
        quantity_scaler: row
            .get("quantityTickSize")
            .and_then(parse_value_f64)
            .unwrap_or(1.0),
    })
}

fn parse_cube_snapshot(market: &CubeMarket, value: &Value) -> Vec<DataEvent> {
    let result = value.get("result").unwrap_or(value);
    let ts_ms = result
        .get("lastTransactTime")
        .and_then(Value::as_u64)
        .map(|ns| ns / 1_000_000)
        .unwrap_or_else(now_ms);
    let levels = result
        .get("levels")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    for level in levels {
        let Some(book_level) = cube_level(level, market) else {
            continue;
        };
        match level.get("side").and_then(Value::as_i64) {
            Some(0) => bids.push(book_level),
            Some(1) => asks.push(book_level),
            _ => {}
        }
    }

    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "cube",
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
        exchange: "cube",
        market: MarketKind::Spot,
        symbol: market.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id: result.get("lastTransactTime").and_then(Value::as_u64),
        ts_ms,
    }));
    events
}

fn cube_level(level: &Value, market: &CubeMarket) -> Option<BookLevel> {
    Some(BookLevel {
        price: level.get("price").and_then(parse_value_f64)? * market.price_scaler,
        qty: level.get("quantity").and_then(parse_value_f64)? * market.quantity_scaler,
    })
}

fn parse_cube_trades(
    market: &CubeMarket,
    value: &Value,
    since_trade_id: Option<u64>,
) -> (Vec<DataEvent>, Option<u64>) {
    let rows = value
        .pointer("/result/trades")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut max_trade_id = since_trade_id;
    let mut events = Vec::new();

    for row in rows {
        let Some(trade_id) = row.get("tradeId").and_then(Value::as_u64) else {
            continue;
        };
        max_trade_id = Some(max_trade_id.map_or(trade_id, |current| current.max(trade_id)));
        if since_trade_id.is_some_and(|seen| trade_id <= seen) {
            continue;
        }
        let Some(price) = row.get("price").and_then(parse_value_f64) else {
            continue;
        };
        let Some(qty) = row.get("fillQuantity").and_then(parse_value_f64) else {
            continue;
        };
        let side = match row.get("aggressingSide").and_then(Value::as_i64) {
            Some(0) => TradeSide::Buy,
            Some(1) => TradeSide::Sell,
            _ => TradeSide::Unknown,
        };
        events.push(DataEvent::Trade(TradeTick {
            exchange: "cube",
            market: MarketKind::Spot,
            symbol: market.symbol.clone().into_boxed_str(),
            price: price * market.price_scaler,
            qty: qty * market.quantity_scaler,
            side,
            trade_id: Some(trade_id.to_string().into_boxed_str()),
            ts_ms: row
                .get("transactTime")
                .and_then(Value::as_u64)
                .map(|ns| ns / 1_000_000)
                .unwrap_or_else(now_ms),
        }));
    }

    (events, max_trade_id)
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
    fn cube_parses_market_metadata() {
        let market = parse_cube_market(&json!({
            "marketId": 100004,
            "symbol": "BTCUSDC",
            "priceTickSize": "0.1",
            "quantityTickSize": "0.00001",
            "status": 1
        }))
        .expect("market");
        assert_eq!(market.market_id, 100004);
        assert_eq!(market.price_scaler, 0.1);
    }

    #[test]
    fn cube_parses_snapshot_as_quote_and_book() {
        let market = CubeMarket {
            market_id: 100004,
            symbol: "BTCUSDC".to_string(),
            price_scaler: 0.1,
            quantity_scaler: 0.00001,
        };
        let events = parse_cube_snapshot(
            &market,
            &json!({
                "result": {
                    "lastTransactTime": 1779292271215584154_u64,
                    "levels": [
                        {"price": 775065, "quantity": 12901, "side": 0},
                        {"price": 775126, "quantity": 16707, "side": 1}
                    ]
                }
            }),
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => assert_eq!(tick.bid, 77506.5),
            other => panic!("unexpected event: {other:?}"),
        }
        match &events[1] {
            DataEvent::OrderBook(book) => assert!((book.asks[0].qty - 0.16707).abs() < 1e-10),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cube_parses_recent_trades_with_dedupe() {
        let market = CubeMarket {
            market_id: 100004,
            symbol: "BTCUSDC".to_string(),
            price_scaler: 0.1,
            quantity_scaler: 0.00001,
        };
        let (events, max_trade_id) = parse_cube_trades(
            &market,
            &json!({
                "result": {
                    "trades": [
                        {"tradeId": 10, "price": 677310, "aggressingSide": 0, "fillQuantity": 922, "transactTime": 1774983523280736003_u64},
                        {"tradeId": 11, "price": 678504, "aggressingSide": 1, "fillQuantity": 3, "transactTime": 1774983618016689462_u64}
                    ]
                }
            }),
            Some(10),
        );

        assert_eq!(max_trade_id, Some(11));
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.symbol.as_ref(), "BTCUSDC");
                assert_eq!(trade.side, TradeSide::Sell);
                assert!((trade.price - 67850.4).abs() < 1e-9);
                assert!((trade.qty - 0.00003).abs() < 1e-12);
                assert_eq!(trade.trade_id.as_deref(), Some("11"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
