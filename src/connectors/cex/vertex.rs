use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::parse_value_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

const VERTEX_WS_URL: &str = "wss://gateway.prod.vertexprotocol.com/v1/subscribe";
const VERTEX_QUERY_URL: &str = "https://gateway.prod.vertexprotocol.com/v1/query";
const VERTEX_ARCHIVE_URL: &str = "https://archive.prod.vertexprotocol.com/v1";

#[derive(Debug, Clone)]
pub struct VertexMarket {
    pub(crate) product_id: u64,
    symbol: String,
    market: MarketKind,
}

impl VertexMarket {
    pub fn new(product_id: u64, symbol: impl Into<String>, market: MarketKind) -> Self {
        Self {
            product_id,
            symbol: symbol.into(),
            market,
        }
    }
}

pub struct VertexFeed {
    markets: Vec<VertexMarket>,
    client: reqwest::Client,
}

impl VertexFeed {
    pub fn new(markets: Vec<VertexMarket>) -> Self {
        Self {
            markets,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for VertexFeed {
    fn name(&self) -> &'static str {
        "vertex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.markets.is_empty() {
            anyhow::bail!("vertex markets empty");
        }

        let (ws, _) = connect_async(VERTEX_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        for market in &self.markets {
            sink.send(Message::Text(
                json!({
                    "method": "subscribe",
                    "stream": {"type": "trade", "product_id": market.product_id},
                    "id": market.product_id
                })
                .to_string(),
            ))
            .await?;
            sink.send(Message::Text(
                json!({
                    "method": "subscribe",
                    "stream": {"type": "book_depth", "product_id": market.product_id},
                    "id": market.product_id
                })
                .to_string(),
            ))
            .await?;
        }

        let market_map = self
            .markets
            .iter()
            .map(|market| (market.product_id, market.clone()))
            .collect::<HashMap<_, _>>();
        let mut ping_tick = interval(Duration::from_secs(15));
        let mut metrics_tick = interval(Duration::from_secs(30));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = metrics_tick.tick() => {
                    match fetch_vertex_metrics(&self.client, &self.markets).await {
                        Ok(events) => {
                            for event in events {
                                ctx.emit(event).await?;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, exchange = "vertex", "metrics poll failed");
                        }
                    }
                }
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(60) {
                        anyhow::bail!("vertex heartbeat timeout");
                    }
                    sink.send(Message::Ping(Vec::new())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "vertex", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("vertex stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_vertex_events(&text, &market_map)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("vertex closed"),
                    }
                }
            }
        }
    }
}

async fn fetch_vertex_metrics(
    client: &reqwest::Client,
    markets: &[VertexMarket],
) -> Result<Vec<DataEvent>> {
    let perp_markets = markets
        .iter()
        .filter(|market| market.market == MarketKind::Perp)
        .cloned()
        .collect::<Vec<_>>();
    if perp_markets.is_empty() {
        return Ok(Vec::new());
    }

    let mut events = Vec::new();

    let products = client
        .post(VERTEX_QUERY_URL)
        .json(&json!({"type": "all_products"}))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_vertex_open_interest(&products, &perp_markets));

    let product_ids = perp_markets
        .iter()
        .map(|market| market.product_id)
        .collect::<Vec<_>>();
    let funding = client
        .post(VERTEX_ARCHIVE_URL)
        .json(&json!({"funding_rates": {"product_ids": product_ids}}))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_vertex_funding_rates(&funding, &perp_markets));

    Ok(events)
}

fn parse_vertex_events(text: &str, markets: &HashMap<u64, VertexMarket>) -> Result<Vec<DataEvent>> {
    if text.contains("\"error\"") {
        anyhow::bail!("vertex error message: {text}");
    }
    let value = serde_json::from_str::<Value>(text)?;
    match value.get("type").and_then(Value::as_str) {
        Some("book_depth") => Ok(parse_vertex_book(&value, markets)),
        Some("trade") => Ok(parse_vertex_trade(&value, markets).into_iter().collect()),
        _ => Ok(Vec::new()),
    }
}

fn parse_vertex_open_interest(value: &Value, markets: &[VertexMarket]) -> Vec<DataEvent> {
    let Some(products) = value
        .pointer("/data/perp_products")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let market_map = markets
        .iter()
        .map(|market| (market.product_id, market))
        .collect::<HashMap<_, _>>();
    products
        .iter()
        .filter_map(|product| {
            let product_id = product.get("product_id").and_then(parse_u64)?;
            let market = market_map.get(&product_id)?;
            let open_interest = product
                .pointer("/state/open_interest")
                .and_then(parse_x18)?;
            let oracle_price = product.get("oracle_price_x18").and_then(parse_x18);
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "vertex",
                symbol: market.symbol.clone().into_boxed_str(),
                open_interest,
                open_interest_value: oracle_price.map(|price| price * open_interest),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn parse_vertex_funding_rates(value: &Value, markets: &[VertexMarket]) -> Vec<DataEvent> {
    markets
        .iter()
        .filter_map(|market| {
            let key = market.product_id.to_string();
            let row = value.get(&key)?;
            Some(DataEvent::FundingRate(FundingRateTick {
                exchange: "vertex",
                symbol: market.symbol.clone().into_boxed_str(),
                funding_rate: row.get("funding_rate_x18").and_then(parse_x18)?,
                next_funding_time_ms: None,
                mark_price: None,
                index_price: None,
                ts_ms: row
                    .get("update_time")
                    .and_then(parse_u64)
                    .map(|seconds| seconds.saturating_mul(1_000))
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_vertex_book(value: &Value, markets: &HashMap<u64, VertexMarket>) -> Vec<DataEvent> {
    let Some(market) = vertex_market(value, markets) else {
        return Vec::new();
    };
    let ts_ms = value
        .get("last_max_timestamp")
        .and_then(parse_vertex_timestamp_ms)
        .unwrap_or_else(now_ms);
    let bids = value
        .get("bids")
        .map(parse_vertex_levels)
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .map(parse_vertex_levels)
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "vertex",
            market: market.market,
            symbol: market.symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "vertex",
        market: market.market,
        symbol: market.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id: value.get("last_max_timestamp").and_then(parse_u64),
        ts_ms,
    }));

    events
}

fn parse_vertex_trade(value: &Value, markets: &HashMap<u64, VertexMarket>) -> Option<DataEvent> {
    let market = vertex_market(value, markets)?;
    Some(DataEvent::Trade(TradeTick {
        exchange: "vertex",
        market: market.market,
        symbol: market.symbol.clone().into_boxed_str(),
        price: value.get("price").and_then(parse_x18).unwrap_or(0.0),
        qty: value.get("taker_qty").and_then(parse_x18).unwrap_or(0.0),
        side: match value.get("is_taker_buyer").and_then(Value::as_bool) {
            Some(true) => TradeSide::Buy,
            Some(false) => TradeSide::Sell,
            None => TradeSide::Unknown,
        },
        trade_id: value
            .get("timestamp")
            .and_then(value_to_string)
            .map(String::into_boxed_str),
        ts_ms: value
            .get("timestamp")
            .and_then(parse_vertex_timestamp_ms)
            .unwrap_or_else(now_ms),
    }))
}

fn vertex_market<'a>(
    value: &Value,
    markets: &'a HashMap<u64, VertexMarket>,
) -> Option<&'a VertexMarket> {
    value
        .get("product_id")
        .and_then(parse_u64)
        .and_then(|product_id| markets.get(&product_id))
}

fn parse_vertex_levels(value: &Value) -> Vec<BookLevel> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Array(pair) => Some(BookLevel {
                price: pair.first().and_then(parse_x18)?,
                qty: pair.get(1).and_then(parse_x18)?,
            }),
            Value::Object(_) => Some(BookLevel {
                price: item
                    .get("price")
                    .or_else(|| item.get("p"))
                    .and_then(parse_x18)?,
                qty: item
                    .get("qty")
                    .or_else(|| item.get("size"))
                    .or_else(|| item.get("q"))
                    .and_then(parse_x18)?,
            }),
            _ => None,
        })
        .collect()
}

fn parse_x18(value: &Value) -> Option<f64> {
    parse_value_f64(value).map(|raw| raw / 1_000_000_000_000_000_000.0)
}

fn parse_u64(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|x| x.parse::<u64>().ok())
        .or_else(|| value.as_u64())
}

fn parse_vertex_timestamp_ms(value: &Value) -> Option<u64> {
    let raw = parse_u64(value)?;
    Some(if raw > 10_000_000_000_000_000 {
        raw / 1_000_000
    } else if raw > 10_000_000_000_000 {
        raw / 1_000
    } else if raw < 10_000_000_000 {
        raw * 1_000
    } else {
        raw
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_u64().map(|x| x.to_string()))
        .or_else(|| value.as_i64().map(|x| x.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn markets() -> HashMap<u64, VertexMarket> {
        [(2, VertexMarket::new(2, "BTC-PERP", MarketKind::Perp))]
            .into_iter()
            .collect()
    }

    #[test]
    fn vertex_parses_book_as_quote_and_book() {
        let text = json!({
            "type": "book_depth",
            "product_id": 2,
            "last_max_timestamp": "1779290460034894378",
            "bids": [["77661500000000000000000", "2665000000000000000"]],
            "asks": [["77661600000000000000000", "6000000000000000"]]
        })
        .to_string();

        let events = parse_vertex_events(&text, &markets()).expect("events");
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.exchange, "vertex");
                assert_eq!(tick.symbol.as_ref(), "BTC-PERP");
                assert_eq!(tick.bid, 77661.5);
                assert_eq!(tick.ask, 77661.6);
                assert_eq!(tick.ts_ms, 1_779_290_460_034);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(matches!(&events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn vertex_parses_trade_side() {
        let text = json!({
            "type": "trade",
            "product_id": 2,
            "timestamp": "1779290460034894378",
            "price": "77659200000000000000000",
            "taker_qty": "4192000000000000000",
            "is_taker_buyer": false
        })
        .to_string();

        let events = parse_vertex_events(&text, &markets()).expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.side, TradeSide::Sell);
                assert!((trade.price - 77659.2).abs() < 0.0000001);
                assert_eq!(trade.qty, 4.192);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn vertex_parses_open_interest_from_all_products() {
        let market = VertexMarket::new(2, "BTC-PERP", MarketKind::Perp);
        let events = parse_vertex_open_interest(
            &json!({
                "status": "success",
                "data": {
                    "perp_products": [{
                        "product_id": 2,
                        "oracle_price_x18": "100000000000000000000000",
                        "state": {
                            "open_interest": "2500000000000000000"
                        }
                    }]
                }
            }),
            &[market],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::OpenInterest(tick) => {
                assert_eq!(tick.exchange, "vertex");
                assert_eq!(tick.symbol.as_ref(), "BTC-PERP");
                assert_eq!(tick.open_interest, 2.5);
                assert!((tick.open_interest_value.expect("oi value") - 250000.0).abs() < 0.0000001);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn vertex_parses_funding_rates() {
        let market = VertexMarket::new(2, "BTC-PERP", MarketKind::Perp);
        let events = parse_vertex_funding_rates(
            &json!({
                "2": {
                    "product_id": 2,
                    "funding_rate_x18": "2447900598160952",
                    "update_time": "1680116326"
                }
            }),
            &[market],
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::FundingRate(tick) => {
                assert_eq!(tick.exchange, "vertex");
                assert_eq!(tick.symbol.as_ref(), "BTC-PERP");
                assert!((tick.funding_rate - 0.002447900598160952).abs() < 0.000000000000001);
                assert_eq!(tick.ts_ms, 1_680_116_326_000);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
