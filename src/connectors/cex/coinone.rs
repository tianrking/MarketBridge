use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const COINONE_REST_URL: &str = "https://api.coinone.co.kr/public/v2";

pub struct CoinoneSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl CoinoneSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinoneSpotFeed {
    fn name(&self) -> &'static str {
        "coinone"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("coinone spot symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_coinone_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "coinone", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "coinone",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_coinone_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let (quote, target) = coinone_pair(symbol)?;
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{COINONE_REST_URL}/ticker_new/{quote}/{target}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinone_ticker(symbol, &ticker));

    let book = client
        .get(format!("{COINONE_REST_URL}/orderbook/{quote}/{target}"))
        .query(&[("size", "15")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinone_book(symbol, &book));

    let trades = client
        .get(format!("{COINONE_REST_URL}/trades/{quote}/{target}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinone_trades(symbol, &trades));

    Ok(events)
}

fn coinone_pair(symbol: &str) -> Result<(String, String)> {
    let (base, quote) = symbol
        .split_once('_')
        .ok_or_else(|| anyhow!("coinone symbol must use BASE_QUOTE form: {symbol}"))?;
    Ok((quote.to_ascii_uppercase(), base.to_ascii_uppercase()))
}

fn parse_coinone_ticker(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value
        .get("tickers")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .unwrap_or(value);
    match (
        row.pointer("/best_bids/0/price").and_then(parse_value_f64),
        row.pointer("/best_asks/0/price").and_then(parse_value_f64),
    ) {
        (Some(bid), Some(ask)) => vec![DataEvent::Tick(MarketTick {
            exchange: "coinone",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_lowercase().into_boxed_str(),
            bid,
            ask,
            mark: row.get("last").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms: row
                .get("timestamp")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms),
        })],
        _ => Vec::new(),
    }
}

fn parse_coinone_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "qty"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "qty"))
        .unwrap_or_default();
    let ts_ms = value
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "coinone",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_lowercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "coinone",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_lowercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<u64>().ok()),
        ts_ms,
    }));

    events
}

fn parse_coinone_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("transactions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "coinone",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_lowercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("qty").and_then(parse_value_f64)?,
                side: match trade.get("is_seller_maker").and_then(Value::as_bool) {
                    Some(true) => TradeSide::Sell,
                    Some(false) => TradeSide::Buy,
                    None => TradeSide::Unknown,
                },
                trade_id: trade
                    .get("id")
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
    fn parses_coinone_public_market_data() {
        let ticker = parse_coinone_ticker(
            "btc_krw",
            &json!({
                "tickers": [{
                    "timestamp": 1779296880001_u64,
                    "last": "114650000.0",
                    "best_bids": [{"price": "114690000.0", "qty": "0.0661"}],
                    "best_asks": [{"price": "114750000.0", "qty": "0.0512"}]
                }]
            }),
        );
        let book = parse_coinone_book(
            "btc_krw",
            &json!({
                "timestamp": 1779296884058_u64,
                "id": "1779296884058001",
                "bids": [{"price": "114680000", "qty": "0.0661"}],
                "asks": [{"price": "114740000", "qty": "0.0513652"}]
            }),
        );
        let trades = parse_coinone_trades(
            "btc_krw",
            &json!({
                "transactions": [{
                    "id": "1779296940031001",
                    "timestamp": 1779296940031_u64,
                    "price": "114710000",
                    "qty": "0.00004358",
                    "is_seller_maker": true
                }]
            }),
        );

        assert_eq!(
            coinone_pair("btc_krw").unwrap(),
            ("KRW".into(), "BTC".into())
        );
        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.side, TradeSide::Sell);
        assert_eq!(trade.trade_id.as_deref(), Some("1779296940031001"));
    }
}
