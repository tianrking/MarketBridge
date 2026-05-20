use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const WOO_REST_URL: &str = "https://api.woox.io/v1/public";

pub struct WooFeed {
    market: MarketKind,
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl WooFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self {
            market,
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for WooFeed {
    fn name(&self) -> &'static str {
        "woo"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("woo symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_woo_market(&self.client, self.market, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(
                        exchange = "woo",
                        symbol,
                        market = ?self.market,
                        error = %err,
                        "poll failed"
                    ),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "woo",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_woo_market(
    client: &reqwest::Client,
    market: MarketKind,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("{WOO_REST_URL}/orderbook/{symbol}"))
        .query(&[("max_level", "50")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_woo_book(market, symbol, &book));

    let trades = client
        .get(format!("{WOO_REST_URL}/market_trades"))
        .query(&[("symbol", symbol), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_woo_trades(market, symbol, &trades));

    if market == MarketKind::Perp {
        let futures = client
            .get(format!("{WOO_REST_URL}/futures/{symbol}"))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_woo_futures(symbol, &futures));
    }

    Ok(events)
}

fn parse_woo_book(market: MarketKind, symbol: &str, value: &Value) -> Vec<DataEvent> {
    let ts_ms = value
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let bids = value
        .get("bids")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "quantity"))
        .unwrap_or_default();
    let asks = value
        .get("asks")
        .and_then(Value::as_array)
        .map(|items| parse_object_levels(items, "price", "quantity"))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "woo",
            market,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "woo",
        market,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_woo_trades(market: MarketKind, symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("rows")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter_map(|(idx, trade)| {
            let ts_ms = trade
                .get("executed_timestamp")
                .and_then(parse_value_f64)
                .map(|secs| (secs * 1000.0) as u64)
                .unwrap_or_else(now_ms);
            Some(DataEvent::Trade(TradeTick {
                exchange: "woo",
                market,
                symbol: trade
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_ascii_uppercase()
                    .into_boxed_str(),
                price: trade.get("executed_price").and_then(parse_value_f64)?,
                qty: trade.get("executed_quantity").and_then(parse_value_f64)?,
                side: trade
                    .get("side")
                    .and_then(Value::as_str)
                    .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                    .unwrap_or(crate::types::TradeSide::Unknown),
                trade_id: Some(format!("{ts_ms}-{idx}").into_boxed_str()),
                ts_ms,
            }))
        })
        .collect()
}

fn parse_woo_futures(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("info").unwrap_or(value);
    let symbol = row
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(symbol)
        .to_ascii_uppercase();
    let ts_ms = value
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::new();

    if let Some(funding_rate) = row.get("est_funding_rate").and_then(parse_value_f64) {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "woo",
            symbol: symbol.clone().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: row
                .get("next_funding_time")
                .and_then(parse_value_f64)
                .map(|ts| ts as u64),
            mark_price: row.get("mark_price").and_then(parse_value_f64),
            index_price: row.get("index_price").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = row.get("open_interest").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "woo",
            symbol: symbol.into_boxed_str(),
            open_interest,
            open_interest_value: None,
            ts_ms,
        }));
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_woo_book_trades_and_futures() {
        let book = parse_woo_book(
            MarketKind::Spot,
            "SPOT_BTC_USDT",
            &json!({
                "timestamp": 1779295142488_u64,
                "asks": [{"price": 77657.55, "quantity": 0.000150}],
                "bids": [{"price": 76744.00, "quantity": 0.000100}]
            }),
        );
        let trades = parse_woo_trades(
            MarketKind::Spot,
            "SPOT_BTC_USDT",
            &json!({
                "rows": [{
                    "symbol": "SPOT_BTC_USDT",
                    "side": "BUY",
                    "executed_price": 77301.88,
                    "executed_quantity": 0.106887,
                    "executed_timestamp": "1779295936.909"
                }]
            }),
        );
        let futures = parse_woo_futures(
            "PERP_BTC_USDT",
            &json!({
                "timestamp": 1779295947174_u64,
                "info": {
                    "symbol": "PERP_BTC_USDT",
                    "mark_price": 77282,
                    "index_price": 77312,
                    "est_funding_rate": 0.00006990,
                    "next_funding_time": 1779321600000_u64,
                    "open_interest": 196.63404
                }
            }),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        assert!(matches!(trades[0], DataEvent::Trade(_)));
        assert!(matches!(futures[0], DataEvent::FundingRate(_)));
        assert!(matches!(futures[1], DataEvent::OpenInterest(_)));
    }
}
