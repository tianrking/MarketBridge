use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::connectors::cex::htx::run_htx;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const HTX_LINEAR_REST_URL: &str = "https://api.hbdm.com";

pub struct HtxPerpBbo {
    pub symbols: Vec<String>,
}
impl HtxPerpBbo {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for HtxPerpBbo {
    fn name(&self) -> &'static str {
        "htx"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_htx(
            "wss://api.hbdm.com/linear-swap-ws",
            self.name(),
            MarketKind::Perp,
            &self.symbols,
            ctx,
        )
        .await
    }
}

pub struct HtxPerpRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl HtxPerpRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for HtxPerpRestFeed {
    fn name(&self) -> &'static str {
        "htx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("htx perp REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_htx_perp_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "htx",
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

async fn poll_htx_perp_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("{HTX_LINEAR_REST_URL}/linear-swap-ex/market/depth"))
        .query(&[
            ("contract_code", symbol),
            ("type", "step0"),
            ("depth", "20"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_perp_book(symbol, &book));

    let trades = client
        .get(format!("{HTX_LINEAR_REST_URL}/linear-swap-ex/market/trade"))
        .query(&[("contract_code", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_perp_trades(symbol, &trades));

    let funding = client
        .get(format!(
            "{HTX_LINEAR_REST_URL}/linear-swap-api/v1/swap_funding_rate"
        ))
        .query(&[("contract_code", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_perp_funding(symbol, &funding));

    let open_interest = client
        .get(format!(
            "{HTX_LINEAR_REST_URL}/linear-swap-api/v1/swap_open_interest"
        ))
        .query(&[("contract_code", symbol), ("contract_type", "swap")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_htx_perp_open_interest(symbol, &open_interest));

    Ok(events)
}

fn parse_htx_perp_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("tick").unwrap_or(value);
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
    let ts_ms = row
        .get("ts")
        .or_else(|| value.get("ts"))
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let normalized = symbol.to_ascii_uppercase();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "htx",
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
        exchange: "htx",
        market: MarketKind::Perp,
        symbol: normalized.into_boxed_str(),
        bids,
        asks,
        last_update_id: row.get("version").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_htx_perp_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let rows = value
        .get("tick")
        .and_then(|tick| tick.get("data"))
        .and_then(Value::as_array)
        .or_else(|| value.get("data").and_then(Value::as_array))
        .map(Vec::as_slice)
        .unwrap_or_default();
    rows.iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "htx",
                market: MarketKind::Perp,
                symbol: trade
                    .get("contract_code")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                price: trade
                    .get("price")
                    .or_else(|| trade.get("trade_price"))
                    .and_then(parse_value_f64)?,
                qty: trade
                    .get("amount")
                    .or_else(|| trade.get("trade_volume"))
                    .and_then(parse_value_f64)?,
                side: trade
                    .get("direction")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("id")
                    .or_else(|| trade.get("trade_id"))
                    .and_then(parse_value_f64)
                    .map(|id| (id as u64).to_string().into_boxed_str()),
                ts_ms: trade
                    .get("ts")
                    .or_else(|| trade.get("created_at"))
                    .or_else(|| trade.get("create_date"))
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_htx_perp_funding(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
    let Some(funding_rate) = row.get("funding_rate").and_then(parse_value_f64) else {
        return Vec::new();
    };
    vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "htx",
        symbol: row
            .get("contract_code")
            .and_then(Value::as_str)
            .unwrap_or(symbol)
            .to_ascii_uppercase()
            .into_boxed_str(),
        funding_rate,
        next_funding_time_ms: row
            .get("next_funding_time")
            .and_then(parse_value_f64)
            .map(|ts| ts as u64),
        mark_price: None,
        index_price: None,
        ts_ms: value
            .get("ts")
            .and_then(parse_value_f64)
            .map(|ts| ts as u64)
            .unwrap_or_else(now_ms),
    })]
}

fn parse_htx_perp_open_interest(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "htx",
                symbol: row
                    .get("contract_code")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                open_interest: row.get("volume").and_then(parse_value_f64)?,
                open_interest_value: row.get("value").and_then(parse_value_f64),
                ts_ms: value
                    .get("ts")
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
    fn parses_htx_perp_book_and_trades() {
        let book = parse_htx_perp_book(
            "BTC-USDT",
            &json!({"tick": {
                "version": 12345_u64,
                "ts": 1779297822639_u64,
                "bids": [[77527.0, 2.0]],
                "asks": [[77527.1, 3.0]]
            }}),
        );
        let trades = parse_htx_perp_trades(
            "BTC-USDT",
            &json!({"tick": {"data": [{
                "id": 987_u64,
                "price": 77510.6,
                "amount": 1.0,
                "direction": "buy",
                "ts": 1779297769043_u64
            }]}}),
        );
        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
    }

    #[test]
    fn parses_htx_perp_funding_and_open_interest() {
        let funding = parse_htx_perp_funding(
            "BTC-USDT",
            &json!({
                "data": {
                    "funding_rate": "0.0001",
                    "next_funding_time": "1603728000000",
                    "contract_code": "BTC-USDT"
                },
                "ts": 1603696494714_u64
            }),
        );
        let oi = parse_htx_perp_open_interest(
            "BTC-USDT",
            &json!({
                "data": [{
                    "volume": 7192610.0,
                    "value": 134654290.332,
                    "contract_code": "BTC-USDT"
                }],
                "ts": 1664336503144_u64
            }),
        );
        assert!(matches!(funding[0], DataEvent::FundingRate(_)));
        assert!(matches!(oi[0], DataEvent::OpenInterest(_)));
    }
}
