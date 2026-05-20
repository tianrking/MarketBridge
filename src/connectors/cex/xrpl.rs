use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::parse_value_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, now_ms};

const XRPL_RPC_URL: &str = "https://xrplcluster.com/";
const BITSTAMP_USD_ISSUER: &str = "rvYAfWj5gh67oV6fW32ZzP3Aw4Eubs59B";

#[derive(Debug, Clone)]
pub struct XrplPair {
    symbol: String,
    base: XrplCurrency,
    quote: XrplCurrency,
}

#[derive(Debug, Clone)]
struct XrplCurrency {
    currency: String,
    issuer: Option<String>,
}

impl XrplPair {
    pub fn xrp_usd(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            base: XrplCurrency {
                currency: "XRP".to_string(),
                issuer: None,
            },
            quote: XrplCurrency {
                currency: "USD".to_string(),
                issuer: Some(BITSTAMP_USD_ISSUER.to_string()),
            },
        }
    }
}

pub struct XrplSpotFeed {
    pairs: Vec<XrplPair>,
    client: reqwest::Client,
}

impl XrplSpotFeed {
    pub fn new(pairs: Vec<XrplPair>) -> Self {
        Self {
            pairs,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for XrplSpotFeed {
    fn name(&self) -> &'static str {
        "xrpl"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.pairs.is_empty() {
            anyhow::bail!("xrpl pairs empty");
        }
        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for pair in &self.pairs {
                match fetch_xrpl_book(&self.client, pair).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "xrpl", symbol = %pair.symbol, error = %err, "poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "xrpl",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn fetch_xrpl_book(client: &reqwest::Client, pair: &XrplPair) -> Result<Vec<DataEvent>> {
    let asks_req = xrpl_book_request(&pair.base, &pair.quote);
    let bids_req = xrpl_book_request(&pair.quote, &pair.base);
    let asks_resp = client
        .post(XRPL_RPC_URL)
        .json(&asks_req)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let bids_resp = client
        .post(XRPL_RPC_URL)
        .json(&bids_req)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let asks = asks_resp
        .pointer("/result/offers")
        .and_then(Value::as_array)
        .context("xrpl asks missing")?;
    let bids = bids_resp
        .pointer("/result/offers")
        .and_then(Value::as_array)
        .context("xrpl bids missing")?;
    Ok(parse_xrpl_book(pair, asks, bids))
}

fn xrpl_book_request(taker_gets: &XrplCurrency, taker_pays: &XrplCurrency) -> Value {
    json!({
        "method": "book_offers",
        "params": [{
            "ledger_index": "current",
            "taker_gets": xrpl_currency_json(taker_gets),
            "taker_pays": xrpl_currency_json(taker_pays),
            "limit": 50
        }]
    })
}

fn xrpl_currency_json(currency: &XrplCurrency) -> Value {
    if currency.currency.eq_ignore_ascii_case("XRP") {
        json!({"currency":"XRP"})
    } else {
        json!({"currency": currency.currency, "issuer": currency.issuer})
    }
}

fn parse_xrpl_book(pair: &XrplPair, asks_raw: &[Value], bids_raw: &[Value]) -> Vec<DataEvent> {
    let asks = asks_raw
        .iter()
        .filter_map(parse_xrpl_ask)
        .collect::<Vec<_>>();
    let bids = bids_raw
        .iter()
        .filter_map(parse_xrpl_bid)
        .collect::<Vec<_>>();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "xrpl",
            market: MarketKind::Spot,
            symbol: pair.symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "xrpl",
        market: MarketKind::Spot,
        symbol: pair.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));
    events
}

fn parse_xrpl_ask(offer: &Value) -> Option<BookLevel> {
    let gets = amount(
        offer
            .get("taker_gets_funded")
            .or_else(|| offer.get("TakerGets"))?,
    )?;
    let pays = amount(
        offer
            .get("taker_pays_funded")
            .or_else(|| offer.get("TakerPays"))?,
    )?;
    Some(BookLevel {
        price: pays / gets,
        qty: gets,
    })
}

fn parse_xrpl_bid(offer: &Value) -> Option<BookLevel> {
    let gets = amount(
        offer
            .get("taker_gets_funded")
            .or_else(|| offer.get("TakerGets"))?,
    )?;
    let pays = amount(
        offer
            .get("taker_pays_funded")
            .or_else(|| offer.get("TakerPays"))?,
    )?;
    Some(BookLevel {
        price: gets / pays,
        qty: pays,
    })
}

fn amount(value: &Value) -> Option<f64> {
    match value {
        Value::String(drops) => drops.parse::<f64>().ok().map(|x| x / 1_000_000.0),
        Value::Object(_) => value.get("value").and_then(parse_value_f64),
        _ => parse_value_f64(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn xrpl_parses_book_offers_as_quote_and_book() {
        let pair = XrplPair::xrp_usd("XRPUSD");
        let events = parse_xrpl_book(
            &pair,
            &[json!({
                "TakerGets": "100000000",
                "TakerPays": {"currency":"USD","issuer":BITSTAMP_USD_ISSUER,"value":"50"}
            })],
            &[json!({
                "TakerGets": {"currency":"USD","issuer":BITSTAMP_USD_ISSUER,"value":"49"},
                "TakerPays": "100000000"
            })],
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.symbol.as_ref(), "XRPUSD");
                assert_eq!(tick.bid, 0.49);
                assert_eq!(tick.ask, 0.5);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
