use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick, TradeTick,
    now_ms,
};

const PHEMEX_REST_URL: &str = "https://api.phemex.com";

pub struct PhemexPerpFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl PhemexPerpFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for PhemexPerpFeed {
    fn name(&self) -> &'static str {
        "phemex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("phemex perp symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_phemex_perp_market(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => warn!(exchange = "phemex", symbol, error = %err, "poll failed"),
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "phemex",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn poll_phemex_perp_market(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let ticker = client
        .get(format!("{PHEMEX_REST_URL}/md/v2/ticker/24hr"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_phemex_ticker(symbol, &ticker));

    let book = client
        .get(format!("{PHEMEX_REST_URL}/md/v2/orderbook"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_phemex_book(symbol, &book));

    let trades = client
        .get(format!("{PHEMEX_REST_URL}/md/v2/trade"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_phemex_trades(symbol, &trades));

    Ok(events)
}

fn parse_phemex_ticker(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("result").unwrap_or(value);
    let symbol = row
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
    let ts_ms = row
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(ns_to_ms)
        .unwrap_or_else(now_ms);
    let bid = row
        .get("bidRp")
        .and_then(parse_value_f64)
        .or_else(|| row.get("closeRp").and_then(parse_value_f64));
    let ask = row
        .get("askRp")
        .and_then(parse_value_f64)
        .or_else(|| row.get("closeRp").and_then(parse_value_f64));
    let mut events = Vec::new();

    if let (Some(bid), Some(ask)) = (bid, ask) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "phemex",
            market: MarketKind::Perp,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: row.get("markPriceRp").and_then(parse_value_f64),
            funding_rate: row.get("fundingRateRr").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(funding_rate) = row.get("fundingRateRr").and_then(parse_value_f64) {
        events.push(DataEvent::FundingRate(FundingRateTick {
            exchange: "phemex",
            symbol: symbol.clone().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: None,
            mark_price: row.get("markPriceRp").and_then(parse_value_f64),
            index_price: row.get("indexPriceRp").and_then(parse_value_f64),
            ts_ms,
        }));
    }

    if let Some(open_interest) = row.get("openInterestRv").and_then(parse_value_f64) {
        events.push(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "phemex",
            symbol: symbol.into_boxed_str(),
            open_interest,
            open_interest_value: None,
            ts_ms,
        }));
    }

    events
}

fn parse_phemex_book(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("result").unwrap_or(value);
    let book = row
        .get("orderbook_p")
        .or_else(|| row.get("book"))
        .unwrap_or(row);
    let symbol = row
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
    let ts_ms = row
        .get("timestamp")
        .and_then(parse_value_f64)
        .map(ns_to_ms)
        .unwrap_or_else(now_ms);
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
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "phemex",
            market: MarketKind::Perp,
            symbol: symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "phemex",
        market: MarketKind::Perp,
        symbol: symbol.into_boxed_str(),
        bids,
        asks,
        last_update_id: row.get("sequence").and_then(Value::as_u64),
        ts_ms,
    }));

    events
}

fn parse_phemex_trades(fallback_symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("result").unwrap_or(value);
    let symbol = row
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or(fallback_symbol)
        .to_ascii_uppercase();
    row.get("trades_p")
        .or_else(|| row.get("trades"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter_map(|(idx, trade)| {
            let fields = trade.as_array()?;
            let ts_ms = fields.first().and_then(parse_value_f64).map(ns_to_ms)?;
            let side = fields
                .get(1)
                .and_then(Value::as_str)
                .map(|side| side_from_labels(side, &["buy"], &["sell"]))
                .unwrap_or(crate::types::TradeSide::Unknown);
            let price = fields.get(2).and_then(parse_value_f64)?;
            let qty = fields.get(3).and_then(parse_value_f64)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "phemex",
                market: MarketKind::Perp,
                symbol: symbol.clone().into_boxed_str(),
                price,
                qty,
                side,
                trade_id: Some(format!("{ts_ms}-{idx}").into_boxed_str()),
                ts_ms,
            }))
        })
        .collect()
}

fn ns_to_ms(ns: f64) -> u64 {
    (ns / 1_000_000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_phemex_ticker_funding_and_oi() {
        let events = parse_phemex_ticker(
            "BTCUSDT",
            &json!({
                "result": {
                    "closeRp": "67550.1",
                    "fundingRateRr": "0.0001",
                    "indexPriceRp": "67567.15389794",
                    "markPriceRp": "67550.1",
                    "openInterestRv": "1848.1144186",
                    "symbol": "BTCUSDT",
                    "timestamp": 1729114315443343001_u64
                }
            }),
        );

        assert!(matches!(events[0], DataEvent::Tick(_)));
        assert!(matches!(events[1], DataEvent::FundingRate(_)));
        let DataEvent::OpenInterest(open_interest) = &events[2] else {
            panic!("expected open interest");
        };
        assert_eq!(open_interest.open_interest, 1848.1144186);
    }

    #[test]
    fn parses_phemex_book_and_trades() {
        let book_events = parse_phemex_book(
            "BTCUSDT",
            &json!({
                "result": {
                    "orderbook_p": {
                        "asks": [["77220", "0.813"]],
                        "bids": [["77219.9", "2.65"]]
                    },
                    "sequence": 61281993768_u64,
                    "symbol": "BTCUSDT",
                    "timestamp": 1779295367771569292_u64
                }
            }),
        );
        let trade_events = parse_phemex_trades(
            "BTCUSDT",
            &json!({
                "result": {
                    "symbol": "BTCUSDT",
                    "trades_p": [[1779295368084817318_u64, "Buy", "77220", "0.001"]]
                }
            }),
        );

        let DataEvent::OrderBook(book) = &book_events[1] else {
            panic!("expected order book");
        };
        assert_eq!(book.bids[0].price, 77219.9);
        assert_eq!(book.asks[0].qty, 0.813);

        let DataEvent::Trade(trade) = &trade_events[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.price, 77220.0);
        assert_eq!(trade.ts_ms, 1779295368084);
    }
}
