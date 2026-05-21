use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::connectors::cex::kraken::run_kraken;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const KRAKEN_FUTURES_REST_URL: &str = "https://futures.kraken.com/derivatives/api/v3";

pub struct KrakenPerpTicker {
    pub symbols: Vec<String>,
}
impl KrakenPerpTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct KrakenPerpRestFeed {
    pub symbols: Vec<String>,
    client: reqwest::Client,
}

impl KrakenPerpRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for KrakenPerpTicker {
    fn name(&self) -> &'static str {
        "kraken"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_kraken(
            "wss://ws.kraken.com/v2",
            self.name(),
            MarketKind::Perp,
            &self.symbols,
            ctx,
        )
        .await
    }
}

#[async_trait]
impl ExchangeSource for KrakenPerpRestFeed {
    fn name(&self) -> &'static str {
        "kraken"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("kraken perp REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_kraken_perp_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => {
                        warn!(exchange = "kraken", symbol, error = %error, "perp REST poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: self.name(),
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_kraken_perp_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{KRAKEN_FUTURES_REST_URL}/tickers"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kraken_perp_tickers(symbol, &ticker));

    let book = client
        .get(format!("{KRAKEN_FUTURES_REST_URL}/orderbook"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kraken_perp_book(symbol, &book));

    let trades = client
        .get(format!("{KRAKEN_FUTURES_REST_URL}/history"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kraken_perp_trades(symbol, &trades));

    Ok(events)
}

fn parse_kraken_perp_tickers(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("tickers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .flat_map(|ticker| {
            let symbol = ticker
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or(symbol);
            let ts_ms = now_ms();
            let bid = ticker.get("bid").and_then(parse_value_f64);
            let ask = ticker.get("ask").and_then(parse_value_f64);
            let mark = ticker.get("markPrice").and_then(parse_value_f64);
            let raw_funding = ticker.get("fundingRate").and_then(parse_value_f64);
            let funding_rate = raw_funding.zip(mark).map(|(rate, mark)| rate / mark);
            let mut events = Vec::new();

            if let (Some(bid), Some(ask)) = (bid, ask) {
                events.push(DataEvent::Tick(MarketTick {
                    exchange: "kraken",
                    market: MarketKind::Perp,
                    symbol: symbol.to_string().into_boxed_str(),
                    bid,
                    ask,
                    mark,
                    funding_rate,
                    ts_ms,
                }));
            }
            if let Some(funding_rate) = funding_rate {
                events.push(DataEvent::FundingRate(FundingRateTick {
                    exchange: "kraken",
                    symbol: symbol.to_string().into_boxed_str(),
                    funding_rate,
                    next_funding_time_ms: None,
                    mark_price: mark,
                    index_price: ticker.get("indexPrice").and_then(parse_value_f64),
                    ts_ms,
                }));
            }
            if let Some(open_interest) = ticker.get("openInterest").and_then(parse_value_f64) {
                events.push(DataEvent::OpenInterest(OpenInterestTick {
                    exchange: "kraken",
                    symbol: symbol.to_string().into_boxed_str(),
                    open_interest,
                    open_interest_value: None,
                    ts_ms,
                }));
            }

            events
        })
        .collect()
}

fn parse_kraken_perp_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let book = value.get("orderBook").unwrap_or(value);
    let bids = book
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = book
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "kraken",
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
        exchange: "kraken",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));
    events
}

fn parse_kraken_perp_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("history")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|trade| trade.get("type").and_then(Value::as_str) == Some("fill"))
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "kraken",
                market: MarketKind::Perp,
                symbol: symbol.to_string().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("size").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("uid")
                    .or_else(|| trade.get("trade_id"))
                    .and_then(|id| {
                        id.as_str()
                            .map(str::to_string)
                            .or_else(|| id.as_i64().map(|n| n.to_string()))
                    })
                    .map(String::into_boxed_str),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_kraken_perp_public_market_data() {
        let ticker = parse_kraken_perp_tickers(
            "PF_XBTUSD",
            &json!({"tickers":[{"symbol":"PF_XBTUSD","markPrice":77995.87215860774,"bid":78000,"ask":78001,"openInterest":1988.5712,"fundingRate":0.163_999_088_464_921_92,"indexPrice":77991.5}]}),
        );
        let book = parse_kraken_perp_book(
            "PF_XBTUSD",
            &json!({"orderBook":{"bids":[[77990.0, 1.5]],"asks":[[78001.0, 2.5]]}}),
        );
        let trades = parse_kraken_perp_trades(
            "PF_XBTUSD",
            &json!({"history":[{"trade_id":100,"price":77983,"size":0.0015,"side":"buy","type":"fill","uid":"abc"}]}),
        );

        assert!(matches!(ticker[0], DataEvent::Tick(_)));
        assert!(matches!(ticker[1], DataEvent::FundingRate(_)));
        assert!(matches!(ticker[2], DataEvent::OpenInterest(_)));
        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        match &trades[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.trade_id.as_deref(), Some("abc")),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
