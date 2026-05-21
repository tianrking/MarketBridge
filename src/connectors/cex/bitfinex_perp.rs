use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::interval;

use crate::connectors::cex::bitfinex::run_bitfinex;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, LiquidationTick, MarketKind, MarketTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

pub struct BitfinexPerpTicker {
    pub symbols: Vec<String>,
}
impl BitfinexPerpTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BitfinexPerpTicker {
    fn name(&self) -> &'static str {
        "bitfinex"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bitfinex(self.name(), MarketKind::Perp, &self.symbols, ctx).await
    }
}

pub struct BitfinexPerpRestFeed {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BitfinexPerpRestFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BitfinexPerpRestFeed {
    fn name(&self) -> &'static str {
        "bitfinex"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            anyhow::bail!("bitfinex perp REST symbols empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_bitfinex_perp_rest(&self.client, symbol).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(error) => tracing::warn!(
                        exchange = "bitfinex",
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

async fn poll_bitfinex_perp_rest(client: &reqwest::Client, symbol: &str) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let book = client
        .get(format!("https://api-pub.bitfinex.com/v2/book/{symbol}/P0"))
        .query(&[("len", "25")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitfinex_perp_book(symbol, &book));

    let trades = client
        .get(format!(
            "https://api-pub.bitfinex.com/v2/trades/{symbol}/hist"
        ))
        .query(&[("limit", "25")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitfinex_perp_trades(symbol, &trades));

    let liquidations = client
        .get("https://api-pub.bitfinex.com/v2/liquidations/hist")
        .query(&[("limit", "100")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_bitfinex_perp_liquidations(symbol, &liquidations));

    Ok(events)
}

fn parse_bitfinex_perp_book(symbol: &str, value: &Value) -> Vec<DataEvent> {
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    for row in value.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(items) = row.as_array() else {
            continue;
        };
        let (Some(price), Some(count), Some(amount)) = (
            items.first().and_then(Value::as_f64),
            items.get(1).and_then(Value::as_f64),
            items.get(2).and_then(Value::as_f64),
        ) else {
            continue;
        };
        if count <= 0.0 || amount == 0.0 {
            continue;
        }
        let level = BookLevel {
            price,
            qty: amount.abs(),
        };
        if amount > 0.0 {
            bids.push(level);
        } else {
            asks.push(level);
        }
    }

    let normalized = symbol.to_ascii_uppercase();
    let ts_ms = now_ms();
    let mut events = Vec::with_capacity(2);
    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "bitfinex",
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
        exchange: "bitfinex",
        market: MarketKind::Perp,
        symbol: normalized.into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));

    events
}

fn parse_bitfinex_perp_trades(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let items = row.as_array()?;
            let amount = items.get(2).and_then(Value::as_f64)?;
            Some(DataEvent::Trade(TradeTick {
                exchange: "bitfinex",
                market: MarketKind::Perp,
                symbol: symbol.to_ascii_uppercase().into_boxed_str(),
                price: items.get(3).and_then(Value::as_f64)?,
                qty: amount.abs(),
                side: if amount > 0.0 {
                    TradeSide::Buy
                } else if amount < 0.0 {
                    TradeSide::Sell
                } else {
                    TradeSide::Unknown
                },
                trade_id: items
                    .first()
                    .and_then(Value::as_i64)
                    .map(|id| id.to_string().into_boxed_str()),
                ts_ms: items
                    .get(1)
                    .and_then(Value::as_f64)
                    .map(|ts| ts as u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_bitfinex_perp_liquidations(symbol: &str, value: &Value) -> Vec<DataEvent> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            let entry = row.as_array()?.first()?.as_array()?;
            let market_id = entry.get(4).and_then(Value::as_str)?;
            if market_id != symbol {
                return None;
            }
            let amount = entry.get(5).and_then(Value::as_f64)?;
            Some(DataEvent::Liquidation(LiquidationTick {
                exchange: "bitfinex",
                symbol: market_id.to_ascii_uppercase().into_boxed_str(),
                side: match entry.get(8).and_then(Value::as_i64) {
                    Some(1) => TradeSide::Buy,
                    Some(_) => TradeSide::Sell,
                    None => {
                        if amount > 0.0 {
                            TradeSide::Buy
                        } else if amount < 0.0 {
                            TradeSide::Sell
                        } else {
                            TradeSide::Unknown
                        }
                    }
                },
                price: entry
                    .get(11)
                    .or_else(|| entry.get(6))
                    .and_then(Value::as_f64)?,
                qty: amount.abs(),
                ts_ms: entry
                    .get(2)
                    .and_then(Value::as_f64)
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
    fn parses_bitfinex_perp_book_trades_and_liquidations() {
        let book = parse_bitfinex_perp_book(
            "tBTCF0:USDTF0",
            &json!([[77527.0, 1, 0.42197508], [77527.1, 1, -0.33297863]]),
        );
        let trades = parse_bitfinex_perp_trades(
            "tBTCF0:USDTF0",
            &json!([[987_i64, 1779297769043_u64, -0.00001, 77510.6]]),
        );
        let liquidations = parse_bitfinex_perp_liquidations(
            "tBTCF0:USDTF0",
            &json!([[[
                "pos",
                171085137,
                1706395919788_u64,
                null,
                "tBTCF0:USDTF0",
                -8,
                32868.0,
                null,
                1,
                1,
                null,
                33255.0
            ]]]),
        );

        assert!(matches!(book[0], DataEvent::Tick(_)));
        assert!(matches!(book[1], DataEvent::OrderBook(_)));
        match &trades[0] {
            DataEvent::Trade(trade) => assert_eq!(trade.side, TradeSide::Sell),
            other => panic!("unexpected event: {other:?}"),
        }
        match &liquidations[0] {
            DataEvent::Liquidation(liq) => {
                assert_eq!(liq.qty, 8.0);
                assert_eq!(liq.price, 33255.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
