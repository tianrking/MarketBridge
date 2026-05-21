use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::parse_value_f64;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const XRPL_RPC_URL: &str = "https://xrplcluster.com/";
const XRPL_WS_URL: &str = "wss://xrplcluster.com/";
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
        let trade_pairs = self.pairs.clone();
        let trade_ctx = ctx.clone();
        tokio::spawn(async move {
            loop {
                if let Err(err) = run_xrpl_trade_stream(&trade_pairs, trade_ctx.clone()).await {
                    warn!(exchange = "xrpl", error = %err, "trade stream stopped");
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });

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

async fn run_xrpl_trade_stream(pairs: &[XrplPair], ctx: SourceContext) -> Result<()> {
    let (ws, _) = connect_async(XRPL_WS_URL).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({
            "command": "subscribe",
            "streams": ["transactions"]
        })
        .to_string(),
    ))
    .await?;

    while let Some(msg) = stream.next().await {
        match msg? {
            Message::Text(text) => {
                for event in parse_xrpl_trade_events(&text, pairs)? {
                    ctx.emit(event).await?;
                }
            }
            Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
            Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
            Message::Close(_) => anyhow::bail!("xrpl trade stream closed"),
        }
    }
    anyhow::bail!("xrpl trade stream ended")
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

fn parse_xrpl_trade_events(text: &str, pairs: &[XrplPair]) -> Result<Vec<DataEvent>> {
    let value = serde_json::from_str::<Value>(text)?;
    if value.get("type").and_then(Value::as_str) != Some("transaction") {
        return Ok(Vec::new());
    }
    if value.get("validated").and_then(Value::as_bool) == Some(false) {
        return Ok(Vec::new());
    }
    let tx = value
        .get("transaction")
        .or_else(|| value.get("tx_json"))
        .unwrap_or(&value);
    let meta = value
        .get("meta")
        .or_else(|| value.get("metaData"))
        .unwrap_or(&value);
    if meta.get("TransactionResult").and_then(Value::as_str) != Some("tesSUCCESS") {
        return Ok(Vec::new());
    }
    let tx_hash = value
        .get("hash")
        .or_else(|| tx.get("hash"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let ts_ms = tx
        .get("date")
        .and_then(parse_value_u64)
        .map(xrpl_epoch_seconds_to_unix_ms)
        .unwrap_or_else(now_ms);

    let Some(nodes) = meta.get("AffectedNodes").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut events = Vec::new();
    for node in nodes {
        let Some(fill) = parse_xrpl_offer_fill(node) else {
            continue;
        };
        for pair in pairs {
            if let Some((price, qty)) = xrpl_pair_trade_price_qty(pair, &fill) {
                events.push(DataEvent::Trade(TradeTick {
                    exchange: "xrpl",
                    market: MarketKind::Spot,
                    symbol: pair.symbol.clone().into_boxed_str(),
                    price,
                    qty,
                    side: TradeSide::Unknown,
                    trade_id: tx_hash.clone().map(String::into_boxed_str),
                    ts_ms,
                }));
            }
        }
    }
    Ok(events)
}

struct XrplOfferFill {
    taker_gets: XrplAmount,
    taker_pays: XrplAmount,
}

#[derive(Debug, Clone)]
struct XrplAmount {
    currency: String,
    issuer: Option<String>,
    value: f64,
}

fn parse_xrpl_offer_fill(node: &Value) -> Option<XrplOfferFill> {
    let (kind, body) = node
        .get("ModifiedNode")
        .map(|body| ("modified", body))
        .or_else(|| node.get("DeletedNode").map(|body| ("deleted", body)))?;
    if body.get("LedgerEntryType").and_then(Value::as_str) != Some("Offer") {
        return None;
    }
    let final_fields = body.get("FinalFields")?;
    let previous_fields = body.get("PreviousFields");
    let final_gets = final_fields.get("TakerGets");
    let final_pays = final_fields.get("TakerPays");
    let previous_gets = previous_fields.and_then(|fields| fields.get("TakerGets"));
    let previous_pays = previous_fields.and_then(|fields| fields.get("TakerPays"));

    let taker_gets = match kind {
        "modified" => xrpl_amount_delta(previous_gets?, final_gets?)?,
        "deleted" => xrpl_amount_from_value(previous_gets.or(final_gets)?)?,
        _ => return None,
    };
    let taker_pays = match kind {
        "modified" => xrpl_amount_delta(previous_pays?, final_pays?)?,
        "deleted" => xrpl_amount_from_value(previous_pays.or(final_pays)?)?,
        _ => return None,
    };
    if taker_gets.value <= 0.0 || taker_pays.value <= 0.0 {
        return None;
    }
    Some(XrplOfferFill {
        taker_gets,
        taker_pays,
    })
}

fn xrpl_amount_delta(previous: &Value, final_value: &Value) -> Option<XrplAmount> {
    let mut previous = xrpl_amount_from_value(previous)?;
    let final_value = xrpl_amount_from_value(final_value)?;
    if !same_xrpl_amount_asset(&previous, &final_value) {
        return None;
    }
    previous.value -= final_value.value;
    Some(previous)
}

fn xrpl_amount_from_value(value: &Value) -> Option<XrplAmount> {
    match value {
        Value::String(drops) => Some(XrplAmount {
            currency: "XRP".to_string(),
            issuer: None,
            value: drops.parse::<f64>().ok()? / 1_000_000.0,
        }),
        Value::Object(_) => Some(XrplAmount {
            currency: value.get("currency")?.as_str()?.to_ascii_uppercase(),
            issuer: value
                .get("issuer")
                .and_then(Value::as_str)
                .map(str::to_string),
            value: value.get("value").and_then(parse_value_f64)?,
        }),
        _ => None,
    }
}

fn xrpl_pair_trade_price_qty(pair: &XrplPair, fill: &XrplOfferFill) -> Option<(f64, f64)> {
    let gets_base = xrpl_amount_matches_currency(&fill.taker_gets, &pair.base);
    let gets_quote = xrpl_amount_matches_currency(&fill.taker_gets, &pair.quote);
    let pays_base = xrpl_amount_matches_currency(&fill.taker_pays, &pair.base);
    let pays_quote = xrpl_amount_matches_currency(&fill.taker_pays, &pair.quote);
    let (base_qty, quote_qty) = if gets_base && pays_quote {
        (fill.taker_gets.value, fill.taker_pays.value)
    } else if gets_quote && pays_base {
        (fill.taker_pays.value, fill.taker_gets.value)
    } else {
        return None;
    };
    if base_qty <= 0.0 || quote_qty <= 0.0 {
        return None;
    }
    Some((quote_qty / base_qty, base_qty))
}

fn xrpl_amount_matches_currency(amount: &XrplAmount, currency: &XrplCurrency) -> bool {
    amount.currency.eq_ignore_ascii_case(&currency.currency)
        && match (&amount.issuer, &currency.issuer) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            (None, None) => true,
            _ => false,
        }
}

fn same_xrpl_amount_asset(left: &XrplAmount, right: &XrplAmount) -> bool {
    left.currency.eq_ignore_ascii_case(&right.currency)
        && match (&left.issuer, &right.issuer) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            (None, None) => true,
            _ => false,
        }
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

fn parse_value_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

fn xrpl_epoch_seconds_to_unix_ms(seconds: u64) -> u64 {
    seconds.saturating_add(946_684_800).saturating_mul(1_000)
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

    #[test]
    fn xrpl_parses_executed_offer_trade() {
        let pair = XrplPair::xrp_usd("XRPUSD");
        let text = json!({
            "type": "transaction",
            "validated": true,
            "transaction": {
                "hash": "ABC",
                "date": 800000000
            },
            "meta": {
                "TransactionResult": "tesSUCCESS",
                "AffectedNodes": [{
                    "ModifiedNode": {
                        "LedgerEntryType": "Offer",
                        "FinalFields": {
                            "TakerGets": "90000000",
                            "TakerPays": {
                                "currency": "USD",
                                "issuer": BITSTAMP_USD_ISSUER,
                                "value": "45"
                            }
                        },
                        "PreviousFields": {
                            "TakerGets": "100000000",
                            "TakerPays": {
                                "currency": "USD",
                                "issuer": BITSTAMP_USD_ISSUER,
                                "value": "50"
                            }
                        }
                    }
                }]
            }
        })
        .to_string();

        let events = parse_xrpl_trade_events(&text, &[pair]).expect("events");
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.exchange, "xrpl");
                assert_eq!(trade.symbol.as_ref(), "XRPUSD");
                assert_eq!(trade.trade_id.as_deref(), Some("ABC"));
                assert_eq!(trade.qty, 10.0);
                assert_eq!(trade.price, 0.5);
                assert_eq!(trade.ts_ms, 1_746_684_800_000);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn xrpl_parses_reverse_offer_trade() {
        let pair = XrplPair::xrp_usd("XRPUSD");
        let text = json!({
            "type": "transaction",
            "validated": true,
            "transaction": {"hash": "DEF"},
            "meta": {
                "TransactionResult": "tesSUCCESS",
                "AffectedNodes": [{
                    "DeletedNode": {
                        "LedgerEntryType": "Offer",
                        "FinalFields": {
                            "TakerGets": {
                                "currency": "USD",
                                "issuer": BITSTAMP_USD_ISSUER,
                                "value": "25"
                            },
                            "TakerPays": "50000000"
                        }
                    }
                }]
            }
        })
        .to_string();

        let events = parse_xrpl_trade_events(&text, &[pair]).expect("events");
        assert_eq!(events.len(), 1);
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.qty, 50.0);
                assert_eq!(trade.price, 0.5);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
