use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{emit_tick_ext, parse_array_levels, parse_value_f64};
use crate::connectors::cex::ws::run_reconnecting;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick, TradeSide, TradeTick,
    now_ms,
};

const BACKPACK_REST_URL: &str = "https://api.backpack.exchange/api/v1";

pub struct BackpackFeed {
    market: MarketKind,
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl BackpackFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self {
            market,
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for BackpackFeed {
    fn name(&self) -> &'static str {
        "backpack"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            bail!("backpack symbols empty");
        }
        let market = self.market;
        let symbols = self.symbols.clone();
        let client = self.client.clone();
        run_reconnecting("backpack", move || {
            let symbols = symbols.clone();
            let ctx = ctx.clone();
            let client = client.clone();
            async move { run_backpack_once(market, &symbols, client, ctx).await }
        })
        .await
    }
}

async fn run_backpack_once(
    market: MarketKind,
    symbols: &[String],
    client: reqwest::Client,
    ctx: SourceContext,
) -> Result<()> {
    let (ws, _) = connect_async("wss://ws.backpack.exchange")
        .await
        .context("backpack connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let streams = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("bookTicker.{symbol}"),
                format!("depth.{symbol}"),
                format!("trade.{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"method":"SUBSCRIBE","params":streams}).to_string(),
    ))
    .await?;
    let mut ping = interval(Duration::from_secs(20));
    let mut rest_tick = interval(Duration::from_secs(30));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    bail!("backpack pong timeout");
                }
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "backpack", ts_ms: now_ms() }).await?;
            }
            _ = rest_tick.tick(), if market == MarketKind::Perp => {
                for symbol in symbols {
                    match fetch_backpack_perp_metrics(&client, symbol).await {
                        Ok(events) => {
                            for event in events {
                                ctx.emit(event).await?;
                            }
                        }
                        Err(err) => warn!(exchange = "backpack", symbol, error = %err, "failed to poll perp metrics"),
                    }
                }
            }
            msg = stream.next() => {
                let msg = msg.context("backpack stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_backpack_events(market, &value, &ctx).await? {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        last_seen = Instant::now();
                        sink.send(Message::Pong(payload)).await?;
                    }
                    Message::Close(_) => bail!("backpack closed"),
                    Message::Pong(_) | Message::Binary(_) => {
                        last_seen = Instant::now();
                    }
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn fetch_backpack_perp_metrics(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Vec<DataEvent>> {
    let mut events = Vec::new();

    let mark_prices = client
        .get(format!("{BACKPACK_REST_URL}/markPrices"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_backpack_mark_prices(symbol, &mark_prices));

    let open_interest = client
        .get(format!("{BACKPACK_REST_URL}/openInterest"))
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    events.extend(parse_backpack_open_interest(symbol, &open_interest));

    Ok(events)
}

async fn parse_backpack_events(
    market: MarketKind,
    value: &Value,
    ctx: &SourceContext,
) -> Result<Vec<DataEvent>> {
    let stream = value
        .get("stream")
        .or_else(|| value.get("e"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value.get("data").unwrap_or(value);
    let symbol = data
        .get("s")
        .or_else(|| data.get("symbol"))
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if stream.contains("bookTicker") {
        let bid = data
            .get("b")
            .or_else(|| data.get("bidPrice"))
            .and_then(Value::as_str)
            .unwrap_or("0");
        let ask = data
            .get("a")
            .or_else(|| data.get("askPrice"))
            .and_then(Value::as_str)
            .unwrap_or("0");
        emit_tick_ext(ctx, "backpack", market, symbol, bid, ask, None, None, None).await?;
        return Ok(Vec::new());
    }
    if stream.contains("depth") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange: "backpack",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bids: data
                .get("b")
                .or_else(|| data.get("bids"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            asks: data
                .get("a")
                .or_else(|| data.get("asks"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            last_update_id: data.get("u").and_then(Value::as_u64),
            ts_ms: data.get("E").and_then(Value::as_u64).unwrap_or_else(now_ms),
        })]);
    }
    if stream.contains("trade") {
        let Some(price) = string_f64(data, "p").or_else(|| string_f64(data, "price")) else {
            return Ok(Vec::new());
        };
        let Some(qty) = string_f64(data, "q").or_else(|| string_f64(data, "quantity")) else {
            return Ok(Vec::new());
        };
        return Ok(vec![DataEvent::Trade(TradeTick {
            exchange: "backpack",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            price,
            qty,
            side: side_from_str(data.get("m").and_then(Value::as_bool)),
            trade_id: data
                .get("t")
                .or_else(|| data.get("tradeId"))
                .and_then(|x| {
                    x.as_i64()
                        .map(|n| n.to_string())
                        .or_else(|| x.as_str().map(str::to_string))
                })
                .map(String::into_boxed_str),
            ts_ms: data.get("T").and_then(Value::as_u64).unwrap_or_else(now_ms),
        })]);
    }
    Ok(Vec::new())
}

fn parse_backpack_mark_prices(symbol: &str, value: &Value) -> Vec<DataEvent> {
    rows(value)
        .into_iter()
        .filter_map(|row| {
            let funding_rate = row.get("fundingRate").and_then(parse_value_f64)?;
            Some(DataEvent::FundingRate(FundingRateTick {
                exchange: "backpack",
                symbol: row
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_string()
                    .into_boxed_str(),
                funding_rate,
                next_funding_time_ms: row.get("nextFundingTimestamp").and_then(Value::as_u64),
                mark_price: row.get("markPrice").and_then(parse_value_f64),
                index_price: row.get("indexPrice").and_then(parse_value_f64),
                ts_ms: now_ms(),
            }))
        })
        .collect()
}

fn parse_backpack_open_interest(symbol: &str, value: &Value) -> Vec<DataEvent> {
    rows(value)
        .into_iter()
        .filter_map(|row| {
            let open_interest = row.get("openInterest").and_then(parse_value_f64)?;
            Some(DataEvent::OpenInterest(OpenInterestTick {
                exchange: "backpack",
                symbol: row
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or(symbol)
                    .to_string()
                    .into_boxed_str(),
                open_interest,
                open_interest_value: None,
                ts_ms: row
                    .get("timestamp")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms),
            }))
        })
        .collect()
}

fn rows(value: &Value) -> Vec<&Value> {
    value
        .as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_else(|| vec![value])
}

fn string_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_str()?.parse::<f64>().ok()
}

fn side_from_str(buyer_is_maker: Option<bool>) -> TradeSide {
    match buyer_is_maker {
        Some(true) => TradeSide::Sell,
        Some(false) => TradeSide::Buy,
        None => TradeSide::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_backpack_mark_prices, parse_backpack_open_interest, side_from_str};
    use crate::types::{DataEvent, TradeSide};
    use serde_json::json;

    #[test]
    fn backpack_side_parser_accepts_maker_flag() {
        assert_eq!(side_from_str(Some(false)), TradeSide::Buy);
        assert_eq!(side_from_str(Some(true)), TradeSide::Sell);
        assert_eq!(side_from_str(None), TradeSide::Unknown);
    }

    #[test]
    fn backpack_parses_public_perp_metrics() {
        let funding = parse_backpack_mark_prices(
            "BTC_USDC_PERP",
            &json!([{
                "fundingRate": "0.0000125",
                "indexPrice": "77923.9191045",
                "markPrice": "77903.9",
                "nextFundingTimestamp": 1779332400000_u64,
                "symbol": "BTC_USDC_PERP"
            }]),
        );
        let open_interest = parse_backpack_open_interest(
            "BTC_USDC_PERP",
            &json!([{
                "openInterest": "417.24604",
                "symbol": "BTC_USDC_PERP",
                "timestamp": 1779332311615_u64
            }]),
        );

        assert!(matches!(funding[0], DataEvent::FundingRate(_)));
        assert!(matches!(open_interest[0], DataEvent::OpenInterest(_)));
    }
}
