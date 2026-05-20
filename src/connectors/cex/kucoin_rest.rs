use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;

use crate::connectors::cex::common::{parse_array_levels, parse_value_f64, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, MarketKind, MarketTick, OrderBookTick, TradeTick, now_ms};

const KUCOIN_REST_URL: &str = "https://api.kucoin.com/api/v1";

pub struct KucoinRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl KucoinRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for KucoinRestFeed {
    fn name(&self) -> &'static str {
        "kucoin"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("kucoin spot REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_kucoin_spot_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => tracing::warn!(
                        exchange = "kucoin",
                        symbol,
                        error = %err,
                        "poll failed"
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

async fn poll_kucoin_spot_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("{KUCOIN_REST_URL}/market/orderbook/level2_20"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kucoin_book(symbol, &book));

    let trades = client
        .get(format!("{KUCOIN_REST_URL}/market/histories"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_kucoin_trades(symbol, &trades));

    Ok(events)
}

fn parse_kucoin_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let row = value.get("data").unwrap_or(value);
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
        .get("time")
        .and_then(parse_value_f64)
        .map(|ts| ts as u64)
        .unwrap_or_else(now_ms);
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "kucoin",
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }

    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "kucoin",
        market: MarketKind::Spot,
        symbol: symbol.to_ascii_uppercase().into_boxed_str(),
        bids,
        asks,
        last_update_id: row
            .get("sequence")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<u64>().ok()),
        ts_ms,
    }));

    events
}

fn parse_kucoin_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|trade| {
            Some(DataEvent::Trade(TradeTick {
                exchange: "kucoin",
                market: MarketKind::Spot,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
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
                    .get("time")
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
    fn parses_kucoin_rest_book_and_trades() {
        let book = parse_kucoin_book(
            "BTC-USDT",
            &json!({"data": {
                "time": 1779297822639_u64,
                "sequence": "32689911009",
                "bids": [["77527", "0.42197508"]],
                "asks": [["77527.1", "0.33297863"]]
            }}),
        );
        let trades = parse_kucoin_trades(
            "BTC-USDT",
            &json!({"data": [{
                "tradeId": "22794279058423808",
                "price": "77510.6",
                "size": "0.00001",
                "side": "buy",
                "time": 1779297769043000000_u64
            }]}),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        let DataEvent::Trade(trade) = &trades[0] else {
            panic!("expected trade");
        };
        assert_eq!(trade.ts_ms, 1_779_297_769_043);
    }
}
