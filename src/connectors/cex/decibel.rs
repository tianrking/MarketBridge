use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

use crate::connectors::cex::common::{parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

const DECIBEL_REST_URL: &str = "https://api.mainnet.aptoslabs.com/decibel";
const DECIBEL_WS_URL: &str = "wss://api.mainnet.aptoslabs.com/decibel/ws";

pub struct DecibelPerpFeed {
    symbols: Vec<String>,
    bearer_token: Option<String>,
    client: reqwest::Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecibelMarket {
    symbol: String,
    address: String,
}

impl DecibelPerpFeed {
    pub fn new(symbols: Vec<String>, bearer_token: Option<String>) -> Self {
        Self {
            symbols,
            bearer_token,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for DecibelPerpFeed {
    fn name(&self) -> &'static str {
        "decibel"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("decibel perp symbols empty");
        }
        let token = self
            .bearer_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .context("decibel bearer token missing; set api_key or api_key_env")?;

        let markets = load_decibel_markets(&self.client, token, &self.symbols).await?;
        let mut request = DECIBEL_WS_URL.into_client_request()?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {token}")
                .parse()
                .context("invalid decibel token header")?,
        );
        let (ws, _) = connect_async(request).await?;
        let (mut sink, mut stream) = ws.split();

        for market in &markets {
            for topic in [
                format!("depth:{}:1", market.address),
                format!("trades:{}", market.address),
                format!("market_price:{}", market.address),
            ] {
                sink.send(Message::Text(
                    json!({"method": "subscribe", "topic": topic}).to_string(),
                ))
                .await?;
            }
        }

        let mut ping_tick = interval(Duration::from_secs(30));
        let mut last_seen = Instant::now();

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    if last_seen.elapsed() > Duration::from_secs(90) {
                        anyhow::bail!("decibel heartbeat timeout");
                    }
                    sink.send(Message::Text(json!({"method": "ping"}).to_string())).await?;
                    ctx.emit(DataEvent::Heartbeat { exchange: "decibel", ts_ms: now_ms() }).await?;
                }
                msg = stream.next() => {
                    let msg = msg.context("decibel stream ended")??;
                    match msg {
                        Message::Text(text) => {
                            last_seen = Instant::now();
                            for event in parse_decibel_events(&text, &markets)? {
                                ctx.emit(event).await?;
                            }
                        }
                        Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                        Message::Pong(_) => last_seen = Instant::now(),
                        Message::Binary(_) | Message::Frame(_) => {}
                        Message::Close(_) => anyhow::bail!("decibel closed"),
                    }
                }
            }
        }
    }
}

async fn load_decibel_markets(
    client: &reqwest::Client,
    token: &str,
    symbols: &[String],
) -> Result<Vec<DecibelMarket>> {
    let mut direct = symbols
        .iter()
        .filter_map(|symbol| configured_decibel_market(symbol))
        .collect::<Vec<_>>();

    let need_discovery = symbols
        .iter()
        .filter(|symbol| configured_decibel_market(symbol).is_none())
        .collect::<Vec<_>>();
    if need_discovery.is_empty() {
        return Ok(direct);
    }

    let value = client
        .get(format!("{DECIBEL_REST_URL}/api/v1/markets"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let rows = market_rows(&value);
    for symbol in need_discovery {
        let wanted = normalize_decibel_symbol(symbol);
        let row = rows
            .iter()
            .find(|row| {
                market_symbols(row)
                    .iter()
                    .any(|s| normalize_decibel_symbol(s) == wanted)
            })
            .with_context(|| format!("decibel market not found for {symbol}"))?;
        let address = first_string(
            row,
            &[
                "market_addr",
                "market_address",
                "marketId",
                "market_id",
                "address",
                "addr",
                "id",
            ],
        )
        .with_context(|| {
            format!(
                "decibel market {symbol} did not expose an address; configure SYMBOL@0x... instead"
            )
        })?;
        direct.push(DecibelMarket {
            symbol: symbol.clone(),
            address: address.to_string(),
        });
    }
    Ok(direct)
}

fn parse_decibel_events(text: &str, markets: &[DecibelMarket]) -> Result<Vec<DataEvent>> {
    let value = serde_json::from_str::<Value>(text)?;
    let Some(topic) = value.get("topic").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let mut parts = topic.split(':');
    let channel = parts.next().unwrap_or_default();
    let address = parts.next().unwrap_or_default();
    let Some(market) = markets
        .iter()
        .find(|market| market.address.eq_ignore_ascii_case(address))
    else {
        return Ok(Vec::new());
    };

    Ok(match channel {
        "depth" => parse_decibel_book(market, &value),
        "trades" => parse_decibel_trades(market, &value),
        "market_price" => parse_decibel_funding(market, &value),
        _ => Vec::new(),
    })
}

fn parse_decibel_book(market: &DecibelMarket, value: &Value) -> Vec<DataEvent> {
    let bids = levels_from_any(
        value
            .get("bids")
            .or_else(|| value.pointer("/data/bids"))
            .or_else(|| value.pointer("/book/bids")),
    );
    let asks = levels_from_any(
        value
            .get("asks")
            .or_else(|| value.pointer("/data/asks"))
            .or_else(|| value.pointer("/book/asks")),
    );
    let ts_ms = first_value(value, &["unix_ms", "timestamp_ms", "ts_ms"])
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "decibel",
            market: MarketKind::Perp,
            symbol: market.symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "decibel",
        market: MarketKind::Perp,
        symbol: market.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id: Some(ts_ms),
        ts_ms,
    }));
    events
}

fn parse_decibel_trades(market: &DecibelMarket, value: &Value) -> Vec<DataEvent> {
    let trades = value
        .get("trades")
        .or_else(|| value.pointer("/data/trades"))
        .or_else(|| value.get("data"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    trades
        .iter()
        .filter_map(|trade| {
            let price = first_value(trade, &["price", "px", "p"]).and_then(parse_value_f64)?;
            let qty =
                first_value(trade, &["size", "qty", "quantity", "q"]).and_then(parse_value_f64)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "decibel",
                market: MarketKind::Perp,
                symbol: market.symbol.clone().into_boxed_str(),
                price,
                qty,
                side: decibel_trade_side(trade),
                trade_id: first_value(trade, &["trade_id", "tradeId", "id"])
                    .map(value_to_string)
                    .map(String::into_boxed_str),
                ts_ms: first_value(trade, &["unix_ms", "timestamp_ms", "ts_ms", "timestamp"])
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_decibel_funding(market: &DecibelMarket, value: &Value) -> Vec<DataEvent> {
    let price = value
        .get("price")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    let Some(funding_bps) =
        first_value(price, &["funding_rate_bps", "fundingRateBps"]).and_then(parse_value_f64)
    else {
        return Vec::new();
    };
    let mut events = vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "decibel",
        symbol: market.symbol.clone().into_boxed_str(),
        funding_rate: funding_bps / 10_000.0,
        next_funding_time_ms: Some(next_hour_ms(now_ms())),
        mark_price: first_value(price, &["mark_px", "mark_price", "markPrice"])
            .and_then(parse_value_f64),
        index_price: first_value(price, &["oracle_px", "index_price", "indexPrice"])
            .and_then(parse_value_f64),
        ts_ms: now_ms(),
    })];
    if let Some(open_interest) = first_value(
        price,
        &[
            "open_interest",
            "openInterest",
            "open_interest_contracts",
            "openInterestContracts",
            "oi",
        ],
    )
    .and_then(parse_value_f64)
    {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "decibel",
            symbol: market.symbol.clone().into_boxed_str(),
            open_interest,
            open_interest_value: first_value(
                price,
                &[
                    "open_interest_value",
                    "openInterestValue",
                    "open_interest_notional",
                    "openInterestNotional",
                ],
            )
            .and_then(parse_value_f64),
            ts_ms: now_ms(),
        }));
    }
    events
}

fn configured_decibel_market(symbol: &str) -> Option<DecibelMarket> {
    if symbol.starts_with("0x") {
        return Some(DecibelMarket {
            symbol: symbol.to_string(),
            address: symbol.to_string(),
        });
    }
    if let Some((label, address)) = symbol
        .split_once('@')
        .filter(|(_, addr)| addr.starts_with("0x"))
    {
        return Some(DecibelMarket {
            symbol: label.to_ascii_uppercase(),
            address: address.to_string(),
        });
    }
    None
}

fn market_rows(value: &Value) -> Vec<&Value> {
    value
        .as_array()
        .map(|items| items.iter().collect())
        .or_else(|| {
            value
                .get("markets")
                .or_else(|| value.get("data"))
                .and_then(Value::as_array)
                .map(|items| items.iter().collect())
        })
        .unwrap_or_default()
}

fn market_symbols(row: &Value) -> Vec<String> {
    [
        "market_name",
        "market",
        "symbol",
        "ticker",
        "name",
        "pair",
        "trading_pair",
        "tradingPair",
    ]
    .iter()
    .filter_map(|key| row.get(*key).and_then(Value::as_str).map(str::to_string))
    .collect()
}

fn levels_from_any(value: Option<&Value>) -> Vec<BookLevel> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let (price, qty) = if let Some(row) = item.as_array() {
                        (row.first(), row.get(1))
                    } else {
                        (
                            first_value(item, &["price", "px", "p"]),
                            first_value(item, &["size", "qty", "quantity", "q"]),
                        )
                    };
                    Some(BookLevel {
                        price: price.and_then(parse_value_f64)?,
                        qty: qty.and_then(parse_value_f64)?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn decibel_trade_side(trade: &Value) -> TradeSide {
    if let Some(side) = first_value(trade, &["side", "direction"]).and_then(Value::as_str) {
        return side_from_labels(
            side,
            &["buy", "bid", "b", "long"],
            &["sell", "ask", "s", "short"],
        );
    }
    if let Some(action) = trade.get("action").and_then(Value::as_str) {
        if action.to_ascii_lowercase().contains("long") {
            return TradeSide::Buy;
        }
        if action.to_ascii_lowercase().contains("short") {
            return TradeSide::Sell;
        }
    }
    match trade.get("is_buy").and_then(Value::as_bool) {
        Some(true) => TradeSide::Buy,
        Some(false) => TradeSide::Sell,
        None => TradeSide::Unknown,
    }
}

fn first_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn first_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(*key))
}

fn value_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn normalize_decibel_symbol(symbol: &str) -> String {
    symbol
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn next_hour_ms(ts_ms: u64) -> u64 {
    ((ts_ms / 3_600_000) + 1) * 3_600_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn market() -> DecibelMarket {
        DecibelMarket {
            symbol: "BTC-USD".to_string(),
            address: "0xmarket".to_string(),
        }
    }

    #[test]
    fn decibel_parses_depth_with_object_and_array_levels() {
        let events = parse_decibel_events(
            &json!({
                "topic": "depth:0xmarket:1",
                "bids": [{"price": "100.1", "size": "2"}],
                "asks": [["100.2", "3"]]
            })
            .to_string(),
            &[market()],
        )
        .expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], DataEvent::Tick(_)));
        match &events[1] {
            DataEvent::OrderBook(book) => {
                assert_eq!(book.bids[0].price, 100.1);
                assert_eq!(book.asks[0].qty, 3.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decibel_parses_trades() {
        let events = parse_decibel_events(
            &json!({
                "topic": "trades:0xmarket",
                "trades": [{
                    "trade_id": 42,
                    "price": "100",
                    "size": "0.5",
                    "action": "open_long",
                    "unix_ms": 1779290460000_u64
                }]
            })
            .to_string(),
            &[market()],
        )
        .expect("events");
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.side, TradeSide::Buy);
                assert_eq!(trade.trade_id.as_deref(), Some("42"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decibel_parses_funding_bps() {
        let events = parse_decibel_events(
            &json!({
                "topic": "market_price:0xmarket",
                "price": {
                    "funding_rate_bps": 5,
                    "mark_px": "50120.5",
                    "oracle_px": "50100",
                    "open_interest": "25",
                    "open_interest_value": "1250000"
                }
            })
            .to_string(),
            &[market()],
        )
        .expect("events");
        match &events[0] {
            DataEvent::FundingRate(funding) => {
                assert_eq!(funding.funding_rate, 0.0005);
                assert_eq!(funding.mark_price, Some(50120.5));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        match &events[1] {
            DataEvent::OpenInterest(oi) => {
                assert_eq!(oi.open_interest, 25.0);
                assert_eq!(oi.open_interest_value, Some(1_250_000.0));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decibel_accepts_direct_market_config() {
        assert_eq!(
            configured_decibel_market("BTC-USD@0xabc"),
            Some(DecibelMarket {
                symbol: "BTC-USD".to_string(),
                address: "0xabc".to_string()
            })
        );
    }
}
