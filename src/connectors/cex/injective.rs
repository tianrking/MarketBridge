use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::time::interval;
use tracing::warn;

use crate::connectors::cex::common::{parse_object_levels, parse_value_f64};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, MarketTick, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

const LCD_BASE: &str = "https://lcd.injective.network/injective/exchange/v1beta1";
const SENTRY_BASE: &str = "https://sentry.exchange.grpc-web.injective.network/api/exchange";

#[derive(Debug, Clone)]
struct InjectiveMarket {
    market_id: String,
    symbol: String,
    market: MarketKind,
    base_decimals: i32,
    quote_decimals: i32,
}

pub struct InjectiveFeed {
    spot_symbols: Vec<String>,
    perp_symbols: Vec<String>,
    client: reqwest::Client,
}

impl InjectiveFeed {
    pub fn new(spot_symbols: Vec<String>, perp_symbols: Vec<String>) -> Self {
        Self {
            spot_symbols,
            perp_symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for InjectiveFeed {
    fn name(&self) -> &'static str {
        "injective"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        let markets =
            resolve_injective_markets(&self.client, &self.spot_symbols, &self.perp_symbols).await?;
        if markets.is_empty() {
            anyhow::bail!("injective markets empty");
        }

        let mut tick = interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            for market in &markets {
                match poll_injective_market(&self.client, market).await {
                    Ok(events) => {
                        for event in events {
                            ctx.emit(event).await?;
                        }
                    }
                    Err(err) => {
                        warn!(exchange = "injective", symbol = %market.symbol, error = %err, "poll failed")
                    }
                }
            }
            ctx.emit(DataEvent::Heartbeat {
                exchange: "injective",
                ts_ms: now_ms(),
            })
            .await?;
        }
    }
}

async fn resolve_injective_markets(
    client: &reqwest::Client,
    spot_symbols: &[String],
    perp_symbols: &[String],
) -> Result<Vec<InjectiveMarket>> {
    let mut markets = Vec::new();
    if !spot_symbols.is_empty() {
        let value = client
            .get(format!("{LCD_BASE}/spot/markets"))
            .query(&[("status", "Active")])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let rows = value
            .get("markets")
            .and_then(Value::as_array)
            .context("injective spot markets missing")?;
        let index = rows
            .iter()
            .filter_map(parse_spot_market)
            .map(|market| (compact_symbol(&market.symbol), market))
            .collect::<HashMap<_, _>>();
        for symbol in spot_symbols {
            if let Some(market) = index.get(&compact_symbol(symbol)) {
                markets.push(market.clone());
            }
        }
    }
    if !perp_symbols.is_empty() {
        let value = client
            .get(format!("{LCD_BASE}/derivative/markets"))
            .query(&[("status", "Active")])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let rows = value
            .get("markets")
            .and_then(Value::as_array)
            .context("injective derivative markets missing")?;
        let mut index = HashMap::new();
        for row in rows {
            if let Some(market) = parse_derivative_market(row) {
                index.insert(compact_symbol(&market.symbol), market.clone());
                let base_only = market
                    .symbol
                    .split('/')
                    .next()
                    .unwrap_or(&market.symbol)
                    .to_ascii_uppercase();
                index
                    .entry(format!("{base_only}USDT"))
                    .or_insert(market.clone());
            }
        }
        for symbol in perp_symbols {
            if let Some(market) = index.get(&compact_symbol(symbol)) {
                markets.push(market.clone());
            }
        }
    }
    Ok(markets)
}

async fn poll_injective_market(
    client: &reqwest::Client,
    market: &InjectiveMarket,
) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();
    let book_path = match market.market {
        MarketKind::Spot => "spot/orderbook",
        MarketKind::Perp => "derivative/orderbook",
    };
    let book = client
        .get(format!("{LCD_BASE}/{book_path}/{}", market.market_id))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_injective_book(market, &book));

    let trade_path = match market.market {
        MarketKind::Spot => "spot/v1/trades",
        MarketKind::Perp => "derivative/v1/trades",
    };
    let trades = client
        .get(format!("{SENTRY_BASE}/{trade_path}"))
        .query(&[("marketId", market.market_id.as_str()), ("limit", "5")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_injective_trades(market, &trades));

    if market.market == MarketKind::Perp {
        let funding = client
            .get(format!("{SENTRY_BASE}/derivative/v1/fundingRates"))
            .query(&[("marketId", market.market_id.as_str()), ("limit", "1")])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_injective_funding(market, &funding));

        let open_interest = client
            .get(format!("{SENTRY_BASE}/derivative/v1/openInterest"))
            .query(&[("marketIDs", market.market_id.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        events.extend(parse_injective_open_interest(market, &open_interest));
    }

    Ok(events)
}

fn parse_spot_market(row: &Value) -> Option<InjectiveMarket> {
    let ticker = row.get("ticker").and_then(Value::as_str)?;
    Some(InjectiveMarket {
        market_id: row.get("market_id").and_then(Value::as_str)?.to_string(),
        symbol: ticker.replace('/', ""),
        market: MarketKind::Spot,
        base_decimals: row
            .get("base_decimals")
            .and_then(Value::as_i64)
            .unwrap_or(18) as i32,
        quote_decimals: row
            .get("quote_decimals")
            .and_then(Value::as_i64)
            .unwrap_or(6) as i32,
    })
}

fn parse_derivative_market(row: &Value) -> Option<InjectiveMarket> {
    let market = row.get("market")?;
    let ticker = market.get("ticker").and_then(Value::as_str)?;
    Some(InjectiveMarket {
        market_id: market.get("market_id").and_then(Value::as_str)?.to_string(),
        symbol: ticker.replace(" PERP", "-PERP"),
        market: MarketKind::Perp,
        base_decimals: 0,
        quote_decimals: market
            .get("quote_decimals")
            .and_then(Value::as_i64)
            .unwrap_or(6) as i32,
    })
}

fn parse_injective_book(market: &InjectiveMarket, value: &Value) -> Vec<DataEvent> {
    let ts_ms = now_ms();
    let bids = value
        .get("buys_price_level")
        .and_then(Value::as_array)
        .map(|items| parse_injective_levels(market, items))
        .unwrap_or_default();
    let asks = value
        .get("sells_price_level")
        .and_then(Value::as_array)
        .map(|items| parse_injective_levels(market, items))
        .unwrap_or_default();
    let mut events = Vec::with_capacity(2);

    if let (Some(bid), Some(ask)) = (
        bids.iter().map(|level| level.price).reduce(f64::max),
        asks.iter().map(|level| level.price).reduce(f64::min),
    ) {
        events.push(DataEvent::Tick(MarketTick {
            exchange: "injective",
            market: market.market,
            symbol: market.symbol.clone().into_boxed_str(),
            bid,
            ask,
            mark: None,
            funding_rate: None,
            ts_ms,
        }));
    }
    events.push(DataEvent::OrderBook(OrderBookTick {
        exchange: "injective",
        market: market.market,
        symbol: market.symbol.clone().into_boxed_str(),
        bids,
        asks,
        last_update_id: None,
        ts_ms,
    }));
    events
}

fn parse_injective_levels(market: &InjectiveMarket, items: &[Value]) -> Vec<BookLevel> {
    parse_object_levels(items, "p", "q")
        .into_iter()
        .map(|level| BookLevel {
            price: normalize_price(market, level.price),
            qty: normalize_qty(market, level.qty),
        })
        .collect()
}

fn parse_injective_trades(market: &InjectiveMarket, value: &Value) -> Vec<DataEvent> {
    let Some(rows) = value.get("trades").and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let (price, qty, side) = match market.market {
                MarketKind::Spot => {
                    let price = row.get("price")?;
                    (
                        price
                            .get("price")
                            .and_then(parse_value_f64)
                            .map(|p| normalize_price(market, p))?,
                        price
                            .get("quantity")
                            .and_then(parse_value_f64)
                            .map(|q| normalize_qty(market, q))?,
                        row.get("tradeDirection")
                            .and_then(Value::as_str)
                            .map(injective_side)?,
                    )
                }
                MarketKind::Perp => {
                    let delta = row.get("positionDelta")?;
                    (
                        delta
                            .get("executionPrice")
                            .and_then(parse_value_f64)
                            .map(|p| normalize_price(market, p))?,
                        delta
                            .get("executionQuantity")
                            .and_then(parse_value_f64)
                            .unwrap_or(0.0),
                        delta
                            .get("tradeDirection")
                            .and_then(Value::as_str)
                            .map(injective_side)?,
                    )
                }
            };
            Some(DataEvent::Trade(TradeTick {
                exchange: "injective",
                market: market.market,
                symbol: market.symbol.clone().into_boxed_str(),
                price,
                qty,
                side,
                trade_id: row
                    .get("tradeId")
                    .and_then(Value::as_str)
                    .map(|x| x.to_string().into_boxed_str()),
                ts_ms: row
                    .get("executedAt")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn parse_injective_funding(market: &InjectiveMarket, value: &Value) -> Vec<DataEvent> {
    let Some(row) = value
        .get("fundingRates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    else {
        return Vec::new();
    };
    let Some(rate) = row.get("rate").and_then(parse_value_f64) else {
        return Vec::new();
    };
    vec![DataEvent::FundingRate(FundingRateTick {
        exchange: "injective",
        symbol: market.symbol.clone().into_boxed_str(),
        funding_rate: rate,
        next_funding_time_ms: None,
        mark_price: None,
        index_price: None,
        ts_ms: row
            .get("timestamp")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms),
    })]
}

fn parse_injective_open_interest(market: &InjectiveMarket, value: &Value) -> Vec<DataEvent> {
    let Some(row) = value
        .get("openInterests")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("marketId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.eq_ignore_ascii_case(&market.market_id))
            })
        })
        .or_else(|| {
            value
                .get("openInterests")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
        })
    else {
        return Vec::new();
    };
    let Some(open_interest) = row.get("openInterest").and_then(parse_value_f64) else {
        return Vec::new();
    };
    vec![DataEvent::OpenInterest(OpenInterestTick {
        exchange: "injective",
        symbol: market.symbol.clone().into_boxed_str(),
        open_interest,
        open_interest_value: None,
        ts_ms: now_ms(),
    })]
}

fn normalize_price(market: &InjectiveMarket, price: f64) -> f64 {
    match market.market {
        MarketKind::Spot => price * 10f64.powi(market.base_decimals - market.quote_decimals),
        MarketKind::Perp => price / 10f64.powi(market.quote_decimals),
    }
}

fn normalize_qty(market: &InjectiveMarket, qty: f64) -> f64 {
    match market.market {
        MarketKind::Spot => qty / 10f64.powi(market.base_decimals),
        MarketKind::Perp => qty,
    }
}

fn injective_side(side: &str) -> TradeSide {
    if side.eq_ignore_ascii_case("buy") {
        TradeSide::Buy
    } else if side.eq_ignore_ascii_case("sell") {
        TradeSide::Sell
    } else {
        TradeSide::Unknown
    }
}

fn compact_symbol(symbol: &str) -> String {
    symbol
        .chars()
        .filter(|ch| !matches!(ch, '-' | '/' | '_' | ' '))
        .collect::<String>()
        .to_ascii_uppercase()
        .replace("PERP", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn perp_market() -> InjectiveMarket {
        InjectiveMarket {
            market_id: "0xabc".to_string(),
            symbol: "BTC/USDC-PERP".to_string(),
            market: MarketKind::Perp,
            base_decimals: 0,
            quote_decimals: 6,
        }
    }

    fn spot_market() -> InjectiveMarket {
        InjectiveMarket {
            market_id: "0xdef".to_string(),
            symbol: "AAVEUSDT".to_string(),
            market: MarketKind::Spot,
            base_decimals: 18,
            quote_decimals: 6,
        }
    }

    #[test]
    fn injective_parses_derivative_book() {
        let events = parse_injective_book(
            &perp_market(),
            &json!({
                "buys_price_level": [{"p":"77466000000.000000000000000000","q":"0.05483"}],
                "sells_price_level": [{"p":"77531000000.000000000000000000","q":"0.00239"}]
            }),
        );
        match &events[0] {
            DataEvent::Tick(tick) => {
                assert_eq!(tick.bid, 77466.0);
                assert_eq!(tick.ask, 77531.0);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn injective_parses_spot_trade_scaling() {
        let events = parse_injective_trades(
            &spot_market(),
            &json!({"trades":[{
                "tradeDirection":"buy",
                "price":{"price":"0.000000000323373","quantity":"40000000000000000"},
                "executedAt":1636375037283u64,
                "tradeId":"4349569_3_0"
            }]}),
        );
        match &events[0] {
            DataEvent::Trade(trade) => {
                assert_eq!(trade.side, TradeSide::Buy);
                assert!((trade.price - 323.373).abs() < 0.000001);
                assert_eq!(trade.qty, 0.04);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn injective_parses_funding() {
        let events = parse_injective_funding(
            &perp_market(),
            &json!({"fundingRates":[{"rate":"0.000038","timestamp":1779289200163u64}]}),
        );
        assert!(matches!(&events[0], DataEvent::FundingRate(_)));
    }

    #[test]
    fn injective_parses_open_interest() {
        let events = parse_injective_open_interest(
            &perp_market(),
            &json!({"openInterests":[{
                "marketId":"0xabc",
                "openInterest":"0.8951"
            }]}),
        );
        match &events[0] {
            DataEvent::OpenInterest(oi) => {
                assert_eq!(oi.exchange, "injective");
                assert_eq!(oi.symbol.as_ref(), "BTC/USDC-PERP");
                assert_eq!(oi.open_interest, 0.8951);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
