use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, MarketKind, MarketTick, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const DEXALOT_REST_URL: &str = "https://api.dexalot.com/privapi/";
const DEXALOT_WS_URL: &str = "wss://api.dexalot.com";

#[derive(Debug, Clone)]
struct DexalotPairRule {
    base_evm_decimals: i32,
    quote_evm_decimals: i32,
    quote_display_decimals: u32,
}

#[derive(Debug, Deserialize)]
struct DexalotPairResponse {
    pair: String,
    status: String,
    base_evmdecimals: i32,
    quote_evmdecimals: i32,
    quotedisplaydecimals: u32,
}

#[derive(Debug, Deserialize)]
struct DexalotMsg {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    pair: Option<String>,
    data: Option<Value>,
}

pub struct DexalotSpotFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl DexalotSpotFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for DexalotSpotFeed {
    fn name(&self) -> &'static str {
        "dexalot"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("dexalot spot symbols empty");
        }
        let rules = fetch_pair_rules(&self.client).await?;

        let (ws, _) = connect_async(DEXALOT_WS_URL).await?;
        let (mut sink, mut stream) = ws.split();
        for symbol in &self.symbols {
            let decimal = rules
                .get(symbol)
                .map(|rule| rule.quote_display_decimals)
                .unwrap_or(4);
            sink.send(Message::Text(
                json!({"type":"subscribe","data":symbol,"pair":symbol,"decimal":decimal})
                    .to_string(),
            ))
            .await?;
        }

        let mut ping_tick = interval(Duration::from_secs(20));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(75) {
                        anyhow::bail!("dexalot spot heartbeat timeout");
                    }
                    ctx.emit(DataEvent::Heartbeat { exchange: "dexalot", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("dexalot spot stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_dexalot_events(&text, &rules)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("dexalot spot closed"),
                    }
                }
            }
        }
    }
}

async fn fetch_pair_rules(client: &reqwest::Client) -> Result<HashMap<String, DexalotPairRule>> {
    let url = format!("{DEXALOT_REST_URL}trading/pairs");
    let rows = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<DexalotPairResponse>>()
        .await
        .context("failed to decode dexalot pair rules")?;

    Ok(rows
        .into_iter()
        .filter(|row| row.status == "deployed")
        .map(|row| {
            (
                row.pair,
                DexalotPairRule {
                    base_evm_decimals: row.base_evmdecimals,
                    quote_evm_decimals: row.quote_evmdecimals,
                    quote_display_decimals: row.quotedisplaydecimals,
                },
            )
        })
        .collect())
}

fn parse_dexalot_events(
    text: &str,
    rules: &HashMap<String, DexalotPairRule>,
) -> Result<Vec<DataEvent>> {
    let msg = serde_json::from_str::<DexalotMsg>(text)?;
    match msg.msg_type.as_deref() {
        Some("orderBooks") => Ok(parse_dexalot_book(msg, rules)),
        Some("lastTrade") => Ok(parse_dexalot_trades(msg, rules)),
        _ => Ok(Vec::new()),
    }
}

fn parse_dexalot_book(msg: DexalotMsg, rules: &HashMap<String, DexalotPairRule>) -> Vec<DataEvent> {
    let symbol = msg.pair.unwrap_or_else(|| "UNKNOWN".to_string());
    let Some(rule) = rules.get(&symbol) else {
        return Vec::new();
    };
    let Some(data) = msg.data else {
        return Vec::new();
    };
    let bids = parse_dexalot_side(data.get("buyBook"), rule);
    let asks = parse_dexalot_side(data.get("sellBook"), rule);
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "dexalot",
            market: MarketKind::Spot,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "dexalot",
        market: MarketKind::Spot,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_dexalot_side(value: Option<&Value>, rule: &DexalotPairRule) -> Vec<BookLevel> {
    let Some(book) = value
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    else {
        return Vec::new();
    };
    let prices = book
        .get("prices")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(',');
    let quantities = book
        .get("quantities")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(',');

    prices
        .zip(quantities)
        .filter_map(|(raw_price, raw_qty)| {
            let price = raw_price.parse::<f64>().ok()? * 10f64.powi(-rule.quote_evm_decimals);
            let qty = raw_qty.parse::<f64>().ok()? * 10f64.powi(-rule.base_evm_decimals);
            Some(BookLevel { price, qty })
        })
        .collect()
}

fn parse_dexalot_trades(
    msg: DexalotMsg,
    rules: &HashMap<String, DexalotPairRule>,
) -> Vec<DataEvent> {
    let symbol = msg.pair.unwrap_or_else(|| "UNKNOWN".to_string());
    let Some(rule) = rules.get(&symbol) else {
        return Vec::new();
    };
    let Some(rows) = msg.data.and_then(|value| value.as_array().cloned()) else {
        return Vec::new();
    };

    rows.iter()
        .filter_map(|row| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "dexalot",
                market: MarketKind::Spot,
                symbol: symbol.clone().into_boxed_str(),
                price: row.get("price").and_then(parse_value_f64).unwrap_or(0.0)
                    * 10f64.powi(-rule.quote_evm_decimals),
                qty: row.get("quantity").and_then(parse_value_f64).unwrap_or(0.0)
                    * 10f64.powi(-rule.base_evm_decimals),
                side: row
                    .get("takerSide")
                    .and_then(Value::as_i64)
                    .map(|side| {
                        if side == 1 {
                            TradeSide::Sell
                        } else {
                            TradeSide::Buy
                        }
                    })
                    .or_else(|| {
                        row.get("side")
                            .and_then(Value::as_str)
                            .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    })
                    .unwrap_or(TradeSide::Unknown),
                trade_id: row
                    .get("execId")
                    .and_then(Value::as_str)
                    .map(|x| x.to_string().into_boxed_str()),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> HashMap<String, DexalotPairRule> {
        HashMap::from([(
            "BTC/USDC".to_string(),
            DexalotPairRule {
                base_evm_decimals: 8,
                quote_evm_decimals: 6,
                quote_display_decimals: 2,
            },
        )])
    }

    #[test]
    fn dexalot_parses_order_book_with_evm_decimals() {
        let text = serde_json::json!({
            "type": "orderBooks",
            "pair": "BTC/USDC",
            "data": {
                "buyBook": [{"prices": "100000000,99000000", "quantities": "200000000,300000000"}],
                "sellBook": [{"prices": "101000000", "quantities": "400000000"}]
            }
        })
        .to_string();

        let events = parse_dexalot_events(&text, &rules()).expect("events");

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::OrderBook(_)));
    }

    #[test]
    fn dexalot_parses_trade_with_evm_decimals() {
        let text = serde_json::json!({
            "type": "lastTrade",
            "pair": "BTC/USDC",
            "data": [{
                "execId": "1",
                "takerSide": 1,
                "price": "100000000",
                "quantity": "200000000"
            }]
        })
        .to_string();

        let events = parse_dexalot_events(&text, &rules()).expect("events");

        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.side, TradeSide::Sell);
                assert_eq!(trade.price, 100.0);
                assert_eq!(trade.qty, 2.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
