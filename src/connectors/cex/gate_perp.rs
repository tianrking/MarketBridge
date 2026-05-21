use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::connectors::cex::gate::run_gate;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, LiquidationTick, MarketKind, MarketTick, OpenInterestTick,
    OrderBookTick, TradeTick, now_ms,
};

const GATE_FUTURES_REST_URL: &str = "https://api.gateio.ws/api/v4/futures/usdt";

pub struct GatePerpBookTicker {
    pub symbols: Vec<String>,
}
impl GatePerpBookTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for GatePerpBookTicker {
    fn name(&self) -> &'static str {
        "gate"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_gate(
            "wss://fx-ws.gateio.ws/v4/ws/usdt",
            "futures.book_ticker",
            "futures.ping",
            self.name(),
            MarketKind::Perp,
            &self.symbols,
            ctx,
        )
        .await
    }
}

pub struct GatePerpRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl GatePerpRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for GatePerpRestFeed {
    fn name(&self) -> &'static str {
        "gate"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("gate perp REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_gate_perp_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "gate",
                        symbol,
                        error = %error,
                        "perp REST poll failed"
                    ),
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

async fn poll_gate_perp_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let contract = client
        .get(format!("{GATE_FUTURES_REST_URL}/contracts/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_perp_contract(symbol, &contract));

    let book = client
        .get(format!("{GATE_FUTURES_REST_URL}/order_book"))
        .query(&[("contract", symbol), ("limit", "20"), ("with_id", "true")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_perp_book(symbol, &book));

    let trades = client
        .get(format!("{GATE_FUTURES_REST_URL}/trades"))
        .query(&[("contract", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_perp_trades(symbol, &trades));

    let liquidations = client
        .get(format!("{GATE_FUTURES_REST_URL}/liq_orders"))
        .query(&[("contract", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_gate_perp_liquidations(symbol, &liquidations));

    Ok(events)
}

fn parse_gate_perp_contract(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let normalized = symbol.to_ascii_uppercase();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(3);

    if let (Some(bid), Some(ask)) = (
        value.get("highest_bid").and_then(parse_value_f64),
        value.get("lowest_ask").and_then(parse_value_f64),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "gate",
            market: MarketKind::Perp,
            symbol: normalized.clone().into_boxed_str(),
            bid,
            ask,
            mark: value.get("mark_price").and_then(parse_value_f64),
            funding_rate: value.get("funding_rate").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(funding_rate) = value.get("funding_rate").and_then(parse_value_f64) {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "gate",
            symbol: normalized.clone().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: None,
            mark_price: value.get("mark_price").and_then(parse_value_f64),
            index_price: value.get("index_price").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = value.get("open_interest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "gate",
            symbol: normalized.into_boxed_str(),
            open_interest,
            open_interest_value: value.get("open_interest_usd").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    events
}

fn parse_gate_perp_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let ts_ms = value
        .get("current")
        .or_else(|| value.get("update"))
        .and_then(parse_value_f64)
        .map(seconds_or_millis)
        .unwrap_or_else(now_ms);
    let normalized = symbol.to_ascii_uppercase();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "gate",
            market: MarketKind::Perp,
            symbol: normalized.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "gate",
        market: MarketKind::Perp,
        symbol: normalized.into_boxed_str(),
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

fn parse_gate_perp_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "gate",
                market: MarketKind::Perp,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade
                    .get("size")
                    .or_else(|| trade.get("amount"))
                    .and_then(parse_value_f64)?
                    .abs(),
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("create_time_ms")
                    .or_else(|| trade.get("create_time"))
                    .and_then(parse_value_f64)
                    .map(seconds_or_millis)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_gate_perp_liquidations(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let size = item.get("size").and_then(parse_value_f64)?;
            Some(DataEvent::Liquidation(LiquidationTick {
                exchange: "gate",
                symbol: item
                    .get("contract")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                side: if size > 0.0 {
                    crate::types::TradeSide::Buy
                } else if size < 0.0 {
                    crate::types::TradeSide::Sell
                } else {
                    crate::types::TradeSide::Unknown
                },
                price: item
                    .get("fill_price")
                    .or_else(|| item.get("liq_price"))
                    .or_else(|| item.get("order_price"))
                    .and_then(parse_value_f64)?,
                qty: size.abs(),
                ts_ms: item
                    .get("time")
                    .and_then(parse_value_f64)
                    .map(seconds_or_millis)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn seconds_or_millis(ts: f64) -> u64 {
    if ts < 10_000_000_000.0 {
        (ts * 1_000.0) as u64
    } else {
        ts as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_gate_perp_contract_market_events() {
        let events = parse_gate_perp_contract(
            "BTC_USDT",
            &json!({
                "highest_bid": "100.1",
                "lowest_ask": "100.2",
                "mark_price": "100.15",
                "index_price": "100.0",
                "funding_rate": "0.0001",
                "open_interest": "2851725",
                "open_interest_usd": "9872386.7775"
            }),
        );
        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::FundingRate(_)));
        assert!(matches!(events[2], DataEvent::OpenInterest(_)));
    }

    #[test]
    fn parses_gate_perp_book_trades_and_liquidations() {
        let book = parse_gate_perp_book(
            "BTC_USDT",
            &json!({
                "id": "2129638396",
                "current": 1779297822.639_f64,
                "bids": [["77527", "2"]],
                "asks": [["77527.1", "3"]]
            }),
        );
        let trades = parse_gate_perp_trades(
            "BTC_USDT",
            &json!([{
                "id": "987",
                "price": "77510.6",
                "size": -5,
                "side": "sell",
                "create_time_ms": "1779297769043"
            }]),
        );
        let liquidations = parse_gate_perp_liquidations(
            "BTC_USDT",
            &json!([{
                "contract": "BTC_USDT",
                "size": -165,
                "fill_price": "28070",
                "time": 1696736132
            }]),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
        match &liquidations[0] {
            DataEvent::Liquidation(liq) => {
                assert_eq!(liq.qty, 165.0);
                assert_eq!(liq.ts_ms, 1_696_736_132_000);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
