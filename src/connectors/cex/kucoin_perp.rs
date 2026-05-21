use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::connectors::cex::kucoin::{KucoinConf, run_kucoin};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const KUCOIN_FUTURES_REST_URL: &str = "https://api-futures.kucoin.com/api/v1";

pub struct KucoinPerpTicker {
    pub symbols: Vec<String>,
    client: reqwest::Client,
}
impl KucoinPerpTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

pub struct KucoinPerpRestFeed {
    pub symbols: Vec<String>,
    client: reqwest::Client,
}

impl KucoinPerpRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for KucoinPerpTicker {
    fn name(&self) -> &'static str {
        "kucoin"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        let conf = KucoinConf {
            bullet_url: "https://api-futures.kucoin.com/api/v1/bullet-public",
            topic_prefix: "/contractMarket/ticker:",
            sub_id_prefix: "sub-perp",
        };
        run_kucoin(
            &conf,
            self.name(),
            MarketKind::Perp,
            &self.symbols,
            ctx,
            &self.client,
        )
        .await
    }
}

#[async_trait]
impl ExchangeSource for KucoinPerpRestFeed {
    fn name(&self) -> &'static str {
        "kucoin"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("kucoin perp REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_kucoin_perp_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => {
                        warn!(exchange = "kucoin", symbol, error = %error, "perp REST poll failed")
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

async fn poll_kucoin_perp_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let contract = client
        .get(format!("{KUCOIN_FUTURES_REST_URL}/contracts/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kucoin_contract(symbol, &contract));

    let book = client
        .get(format!("{KUCOIN_FUTURES_REST_URL}/level2/depth20"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kucoin_perp_book(symbol, &book));

    let trades = client
        .get(format!("{KUCOIN_FUTURES_REST_URL}/trade/history"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kucoin_perp_trades(symbol, &trades));

    let funding = client
        .get(format!(
            "{KUCOIN_FUTURES_REST_URL}/funding-rate/{symbol}/current"
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(event) = parse_kucoin_funding(symbol, &funding) {
        events.push(event);
    }

    Ok(events)
}

fn parse_kucoin_contract(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let data = value.get("data").unwrap_or(value);
    let symbol = data.get("symbol").and_then(Value::as_str).unwrap_or(symbol);
    let ts_ms = now_ms();
    let mut events = Vec::new();

    let bid = data.get("lastTradePrice").and_then(parse_value_f64);
    let ask = bid;
    if let (Some(bid), Some(ask)) = (bid, ask) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "kucoin",
            market: MarketKind::Perp,
            symbol: symbol.to_string().into_boxed_str(),
            bid,
            ask,
            mark: data.get("markPrice").and_then(parse_value_f64),
            funding_rate: data.get("fundingFeeRate").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = data.get("openInterest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "kucoin",
            symbol: symbol.to_string().into_boxed_str(),
            open_interest,
            open_interest_value: None,
            ts_ms,
        }));
    }

    events
}

fn parse_kucoin_funding(symbol: &str, value: &Value) -> Option<DataEvent> {
    let data = value.get("data").unwrap_or(value);
    let funding_rate = data
        .get("value")
        .or_else(|| data.get("nextFundingRate"))
        .and_then(parse_value_f64)?;
    Some(DataEvent::FundingRate(FundingRateTick {
        exchange: "kucoin",
        symbol: symbol.to_string().into_boxed_str(),
        funding_rate,
        next_funding_time_ms: data.get("fundingTime").and_then(Value::as_u64),
        mark_price: None,
        index_price: None,
        ts_ms: data
            .get("timePoint")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms),
    }))
}

fn parse_kucoin_perp_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let data = value.get("data").unwrap_or(value);
    let bids = data
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = data
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = data
        .get("ts")
        .and_then(parse_value_f64)
        .map(nanos_to_millis)
        .unwrap_or_else(now_ms);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "kucoin",
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
        exchange: "kucoin",
        market: MarketKind::Perp,
        symbol: symbol.to_string().into_boxed_str(),
        bids,
        asks,
        last_update_id: data.get("sequence").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_kucoin_perp_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "kucoin",
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
                    .get("tradeId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("ts")
                    .and_then(parse_value_f64)
                    .map(nanos_to_millis)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn nanos_to_millis(ts: f64) -> u64 {
    if ts > 10_000_000_000_000.0 {
        (ts / 1_000_000.0) as u64
    } else {
        ts as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_kucoin_perp_contract_funding_book_and_trades() {
        let contract = parse_kucoin_contract(
            "XBTUSDTM",
            &json!({"data":{"symbol":"XBTUSDTM","fundingFeeRate":-0.000003,"openInterest":"26124343","markPrice":77974.23,"lastTradePrice":77976.4}}),
        );
        let funding = parse_kucoin_funding(
            "XBTUSDTM",
            &json!({"data":{"symbol":".XBTUSDTMFPI8H","timePoint":1779321600000_u64,"value":-0.000003,"fundingTime":1779350400000_u64}}),
        )
        .expect("funding");
        let book = parse_kucoin_perp_book(
            "XBTUSDTM",
            &json!({"data":{"sequence":1742113286964_u64,"bids":[[78067.7,405]],"asks":[[78067.8,1017]],"ts":1779330202593000000_u64}}),
        );
        let trades = parse_kucoin_perp_trades(
            "XBTUSDTM",
            &json!({"data":[{"tradeId":"1932277887415","ts":1779330197058000000_u64,"size":90,"price":"78078","side":"buy"}]}),
        );

        assert!(matches!(contract[0], DataEvent::Tick(_)));
        assert!(matches!(contract[1], DataEvent::OpenInterest(_)));
        assert!(matches!(funding, DataEvent::FundingRate(_)));
        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        match &trades[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.ts_ms, 1_779_330_197_057),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
