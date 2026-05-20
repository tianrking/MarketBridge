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

const CRYPTOCOM_REST_URL: &str = "https://api.crypto.com/exchange/v1";

pub struct CryptoComFeed {
    market: MarketKind,
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl CryptoComFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self {
            market,
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CryptoComFeed {
    fn name(&self) -> &'static str {
        "cryptocom"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("cryptocom symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_cryptocom_market(&self.client, self.market, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(
                        exchange = "cryptocom",
                        symbol,
                        market = ?self.market,
                        error = %err,
                        "poll failed"
                    ),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "cryptocom",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_cryptocom_market(
    client: &reqwest::Client,
    market: MarketKind,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{CRYPTOCOM_REST_URL}/public/get-tickers"))
        .query(&[("instrument_name", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_cryptocom_ticker(market, symbol, &ticker));

    let book = client
        .get(format!("{CRYPTOCOM_REST_URL}/public/get-book"))
        .query(&[("instrument_name", symbol), ("depth", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_cryptocom_book(market, symbol, &book));

    let trades = client
        .get(format!("{CRYPTOCOM_REST_URL}/public/get-trades"))
        .query(&[("instrument_name", symbol), ("count", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_cryptocom_trades(market, symbol, &trades));

    if market == MarketKind::Perp {
        let funding = client
            .get(format!("{CRYPTOCOM_REST_URL}/public/get-valuations"))
            .query(&[
                ("instrument_name", symbol),
                ("valuation_type", "estimated_funding_rate"),
                ("count", "1"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_cryptocom_funding(symbol, &funding));
    }

    Ok(events)
}

fn parse_cryptocom_ticker(
    market: MarketKind,
    fallback_symbol: &str,
    value: &Value,
) -> Vec<DataEvent> {
    result_data(value)
        .into_iter()
        .filter_map(|ticker| {
            let symbol = ticker
                .get("i")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            let bid = ticker.get("b").and_then(parse_value_f64)?;
            let ask = ticker.get("k").and_then(parse_value_f64)?;
            let ts_ms = ticker
                .get("t")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms);
            Some(DataEvent::Tick(MarketTick {
                exchange: "cryptocom",
                market,
                symbol: canonical_cryptocom_symbol(symbol).into_boxed_str(),
                bid,
                ask,
                mark: ticker.get("a").and_then(parse_value_f64),
                funding_rate: None,
                ts_ms,
            }))
        })
        .chain(result_data(value).into_iter().filter_map(|ticker| {
            let oi = ticker.get("oi").and_then(parse_value_f64)?;
            let symbol = ticker
                .get("i")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            let ts_ms = ticker
                .get("t")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64)
                .unwrap_or_else(now_ms);
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "cryptocom",
                symbol: canonical_cryptocom_symbol(symbol).into_boxed_str(),
                open_interest: oi,
                open_interest_value: None,
                ts_ms,
            }))
        }))
        .collect()
}

fn parse_cryptocom_book(
    market: MarketKind,
    fallback_symbol: &str,
    value: &Value,
) -> Vec<DataEvent> {
    let row = result_data(value).into_iter().next().unwrap_or(value);
    let symbol = value
        .pointer("/result/instrument_name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol);
    let ts_ms = row
        .get("t")
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
    let canonical = canonical_cryptocom_symbol(symbol);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "cryptocom",
            market,
            symbol: canonical.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "cryptocom",
        market,
        symbol: canonical.into_boxed_str(),
        bids,
        asks,
        last_update_id: Some(ts_ms),
        ts_ms,
    }));

    events
}

fn parse_cryptocom_trades(
    market: MarketKind,
    fallback_symbol: &str,
    value: &Value,
) -> Vec<DataEvent> {
    result_data(value)
        .into_iter()
        .filter_map(|trade| {
            let symbol = trade
                .get("i")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Trade(TradeTick {
                exchange: "cryptocom",
                market,
                symbol: canonical_cryptocom_symbol(symbol).into_boxed_str(),
                price: trade.get("p").and_then(parse_value_f64)?,
                qty: trade.get("q").and_then(parse_value_f64)?,
                side: trade
                    .get("s")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("d")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("t")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_cryptocom_funding(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let Some(row) = result_data(value).into_iter().next() else {
        return Vec::new();
    };
    let Some(rate) = row.get("v").and_then(parse_value_f64) else {
        return Vec::new();
    };
    let ts_ms = row
        .get("t")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);

    vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "cryptocom",
        symbol: canonical_cryptocom_symbol(symbol).into_boxed_str(),
        funding_rate: rate,
        next_funding_time_ms: Some(next_hour_ms(ts_ms)),
        mark_price: None,
        index_price: None,
        ts_ms,
    })]
}

fn result_data(value: &Value) -> Vec<&Value> {
    value
        .pointer("/result/data")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn canonical_cryptocom_symbol(symbol: &str) -> String {
    symbol.replace(['_', '-', '/'], "").to_ascii_uppercase()
}

fn next_hour_ms(ts_ms: u64) -> u64 {
    ((ts_ms / 3_600_000) + 1) * 3_600_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cryptocom_parses_ticker_and_open_interest() {
        let events = parse_cryptocom_ticker(
            MarketKind::Perp,
            "BTCUSD-PERP",
            &json!({
                "result": {
                    "data": [{
                        "i": "BTCUSD-PERP",
                        "a": "30446.00",
                        "b": "30442.00",
                        "k": "30447.66",
                        "oi": "10888.9",
                        "t": 1687403045415_u64
                    }]
                }
            }),
        );
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        assert!(matches!(&events[1], DataEvent::OpenInterest(_)));
    }

    #[test]
    fn cryptocom_parses_book_as_quote_and_book() {
        let events = parse_cryptocom_book(
            MarketKind::Spot,
            "BTC_USDT",
            &json!({
                "result": {
                    "instrument_name": "BTC_USDT",
                    "data": [{
                        "bids": [["30025.00", "0.00004", "1"]],
                        "asks": [["30025.01", "0.04090", "1"]],
                        "t": 1687491287380_u64
                    }]
                }
            }),
        );
        assert_eq!(events.len(), 2);
        match &events[1] {
            DataEvent::OrderBook(book) => assert_eq!(book.symbol.as_ref(), "BTCUSDT"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cryptocom_parses_trades() {
        let events = parse_cryptocom_trades(
            MarketKind::Spot,
            "BTC_USDT",
            &json!({
                "result": {
                    "data": [{
                        "s": "sell",
                        "p": "26386.00",
                        "q": "0.00453",
                        "t": 1686944282062_u64,
                        "d": "4611686018455979970",
                        "i": "BTC_USDT"
                    }]
                }
            }),
        );
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.trade_id.as_deref(), Some("4611686018455979970"))
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cryptocom_parses_funding() {
        let events = parse_cryptocom_funding(
            "BTCUSD-PERP",
            &json!({
                "result": {
                    "data": [{
                        "v": "-0.000001884",
                        "t": 1687892400000_u64
                    }]
                }
            }),
        );
        match &events[0] {
            DataEvent::FundingRate(funding) => {
                assert_eq!(funding.symbol.as_ref(), "BTCUSDPERP");
                assert_eq!(funding.next_funding_time_ms, Some(1687896000000));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
