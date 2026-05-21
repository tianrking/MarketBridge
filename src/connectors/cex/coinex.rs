use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, LiquidationTick, MarketKind, MarketTick, OpenInterestTick,
    OrderBookTick, TradeSide, TradeTick, now_ms,
};

const COINEX_REST_URL: &str = "https://api.coinex.com/v2";

pub struct CoinexFeed {
    market: MarketKind,
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl CoinexFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self {
            market,
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for CoinexFeed {
    fn name(&self) -> &'static str {
        "coinex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("coinex symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_coinex_market(&self.client, self.market, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(
                        exchange = "coinex",
                        symbol,
                        market = ?self.market,
                        error = %err,
                        "poll failed"
                    ),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "coinex",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_coinex_market(
    client: &reqwest::Client,
    market: MarketKind,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();
    let prefix = coinex_prefix(market);

    let ticker = client
        .get(format!("{COINEX_REST_URL}/{prefix}/ticker"))
        .query(&[("market", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinex_ticker(market, symbol, &ticker));

    let book = client
        .get(format!("{COINEX_REST_URL}/{prefix}/depth"))
        .query(&[("market", symbol), ("limit", "50"), ("interval", "0")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinex_book(market, symbol, &book));

    let trades = client
        .get(format!("{COINEX_REST_URL}/{prefix}/deals"))
        .query(&[("market", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_coinex_trades(market, symbol, &trades));

    if market == MarketKind::Perp {
        let funding = client
            .get(format!("{COINEX_REST_URL}/futures/funding-rate"))
            .query(&[("market", symbol)])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_coinex_funding(symbol, &funding));

        let market_meta = client
            .get(format!("{COINEX_REST_URL}/futures/market"))
            .query(&[("market", symbol)])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_coinex_open_interest(symbol, &market_meta));

        let liquidations = client
            .get(format!("{COINEX_REST_URL}/futures/liquidation-history"))
            .query(&[("market", symbol), ("limit", "20")])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_coinex_liquidations(symbol, &liquidations));
    }

    Ok(events)
}

fn coinex_prefix(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "futures",
    }
}

fn parse_coinex_ticker(market: MarketKind, fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    response_data(value)
        .into_iter()
        .filter_map(|ticker| {
            let symbol = ticker
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            let last = ticker.get("last").and_then(parse_value_f64)?;
            Some(DataEvent::Tick(MarketTick {
                exchange: "coinex",
                market,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                bid: last,
                ask: last,
                mark: ticker
                    .get("mark_price")
                    .and_then(parse_value_f64)
                    .or_else(|| ticker.get("close").and_then(parse_value_f64)),
                funding_rate: None,
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn parse_coinex_book(market: MarketKind, fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
    let depth = row.get("depth").unwrap_or(row);
    let symbol = row
        .get("market")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol);
    let ts_ms = depth
        .get("updated_at")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = depth
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let asks = depth
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_array_levels(items))
        .unwrap_or_default();
    let canonical = symbol.to_ascii_uppercase();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "coinex",
            market,
            symbol: canonical.clone().into_boxed_str(),
            bid,
            ask,
            mark: depth.get("last").and_then(parse_value_f64),
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "coinex",
        market,
        symbol: canonical.into_boxed_str(),
        bids,
        asks,
        last_update_id: depth.get("checksum").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_coinex_trades(market: MarketKind, fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    response_data(value)
        .into_iter()
        .filter_map(|trade| {
            let symbol = trade
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Trade(TradeTick {
                exchange: "coinex",
                market,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: trade.get("price").and_then(parse_value_f64)?,
                qty: trade.get("amount").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: trade
                    .get("deal_id")
                    .and_then(|value| {
                        value
                            .as_i64()
                            .map(|id| id.to_string())
                            .or_else(|| value.as_str().map(str::to_string))
                    })
                    .map(String::into_boxed_str),
                ts_ms: trade
                    .get("created_at")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_coinex_funding(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    response_data(value)
        .into_iter()
        .filter_map(|row| {
            let symbol = row
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::FundingRate(FundingRateTick {
                exchange: "coinex",
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                funding_rate: row.get("latest_funding_rate").and_then(parse_value_f64)?,
                next_funding_time_ms: row
                    .get("next_funding_time")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64),
                mark_price: row.get("mark_price").and_then(parse_value_f64),
                index_price: None,
                ts_ms: row
                    .get("latest_funding_time")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_coinex_open_interest(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    response_data(value)
        .into_iter()
        .filter_map(|row| {
            let symbol = row
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "coinex",
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                open_interest: row.get("open_interest_volume").and_then(parse_value_f64)?,
                open_interest_value: None,
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn parse_coinex_liquidations(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    response_data(value)
        .into_iter()
        .filter_map(|row| {
            let symbol = row
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or(fallback_symbol);
            Some(DataEvent::Liquidation(LiquidationTick {
                exchange: "coinex",
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                side: coinex_liquidation_side(row.get("side").and_then(Value::as_str)),
                price: row
                    .get("liq_price")
                    .or_else(|| row.get("bkr_price"))
                    .and_then(parse_value_f64)?,
                qty: row.get("liq_amount").and_then(parse_value_f64)?,
                ts_ms: row
                    .get("created_at")
                    .and_then(parse_value_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn coinex_liquidation_side(side: Option<&str>) -> TradeSide {
    match side.map(str::to_ascii_lowercase).as_deref() {
        Some("short") | Some("buy") => TradeSide::Buy,
        Some("long") | Some("sell") => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

fn response_data(value: &Value) -> Vec<&Value> {
    match value.get("data") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(item @ Value::Object(_)) => vec![item],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_coinex_book() {
        let events = parse_coinex_book(
            MarketKind::Spot,
            "BTCUSDT",
            &json!({
                "data": {
                    "market": "BTCUSDT",
                    "depth": {
                        "bids": [["70855.3", "0.00632222"]],
                        "asks": [["70875.31", "0.28670282"]],
                        "checksum": 2313816665_u64,
                        "last": "70857.19",
                        "updated_at": 1712823790987_u64
                    }
                }
            }),
        );

        assert!(matches!(events[0], DataEvent::Tick(_)));
        let DataEvent::OrderBook(book) = &events[1] else {
            panic!("expected order book");
        };
        assert_eq!(book.symbol.as_ref(), "BTCUSDT");
        assert_eq!(book.bids[0].price, 70855.3);
        assert_eq!(book.asks[0].qty, 0.28670282);
        assert_eq!(book.last_update_id, Some(2313816665));
    }

    #[test]
    fn parses_coinex_funding_and_open_interest() {
        let funding = parse_coinex_funding(
            "BTCUSDT",
            &json!({
                "data": [{
                    "latest_funding_rate": "0.0001",
                    "latest_funding_time": 1715731200000_u64,
                    "mark_price": "61602.22",
                    "market": "BTCUSDT",
                    "next_funding_time": 1715760000000_u64
                }]
            }),
        );
        let oi = parse_coinex_open_interest(
            "BTCUSDT",
            &json!({
                "data": [{
                    "market": "BTCUSDT",
                    "open_interest_volume": "120.5"
                }]
            }),
        );

        let DataEvent::FundingRate(rate) = &funding[0] else {
            panic!("expected funding rate");
        };
        assert_eq!(rate.funding_rate, 0.0001);
        assert_eq!(rate.next_funding_time_ms, Some(1715760000000));

        let DataEvent::OpenInterest(open_interest) = &oi[0] else {
            panic!("expected open interest");
        };
        assert_eq!(open_interest.open_interest, 120.5);
    }

    #[test]
    fn parses_coinex_liquidations() {
        let events = parse_coinex_liquidations(
            "BTCUSDT",
            &json!({
                "data": [{
                    "market": "BTCUSDT",
                    "side": "short",
                    "liq_price": "78167.66607905355114133432",
                    "bkr_price": "78558.504409448818897041",
                    "liq_amount": "0.0127",
                    "created_at": 1779326151293_u64
                }]
            }),
        );

        let DataEvent::Liquidation(liquidation) = &events[0] else {
            panic!("expected liquidation");
        };
        assert_eq!(liquidation.exchange, "coinex");
        assert_eq!(liquidation.side, TradeSide::Buy);
        assert_eq!(liquidation.price, 78167.66607905355);
        assert_eq!(liquidation.qty, 0.0127);
    }
}
