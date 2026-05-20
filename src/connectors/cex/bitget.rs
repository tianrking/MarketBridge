use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::emit_tick_ext;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, MarketKind, OpenInterestTick, OrderBookTick, TradeSide,
    TradeTick, now_ms,
};

// ── Shared parsing ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BitgetMsg {
    #[serde(default)]
    op: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    arg: Option<BitgetArg>,
    #[serde(default)]
    data: Vec<Value>,
}

#[derive(Deserialize)]
struct BitgetArg {
    #[serde(rename = "instId")]
    inst_id: Option<String>,
    channel: Option<String>,
}

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_bitget(
    inst_type: &str,
    exchange: &'static str,
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let label = if market == MarketKind::Spot {
        "spot"
    } else {
        "perp"
    };
    if symbols.is_empty() {
        anyhow::bail!("bitget {label} symbols empty");
    }

    let (ws, _) = connect_async("wss://ws.bitget.com/v2/ws/public").await?;
    let (mut sink, mut stream) = ws.split();

    let channels = ["ticker", "books5", "trade"];
    let args = symbols
        .iter()
        .flat_map(|s| {
            channels
                .iter()
                .map(move |channel| json!({"instType": inst_type, "channel": channel, "instId": s}))
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"op":"subscribe","args":args}).to_string().into(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(25));
    let mut last_seen = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    anyhow::bail!("bitget {label} heartbeat timeout");
                }
                sink.send(Message::Text("ping".into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("bitget {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        if t == "pong" { last_seen = Instant::now(); continue; }
                        if let Ok(m) = serde_json::from_str::<BitgetMsg>(&t) {
                            if m.op.as_deref() == Some("pong") || m.action.as_deref() == Some("pong") {
                                last_seen = Instant::now();
                                continue;
                            }
                            let arg = m.arg;
                            let arg_inst = arg.as_ref().and_then(|a| a.inst_id.as_deref());
                            let channel = arg.as_ref().and_then(|a| a.channel.as_deref()).unwrap_or_default();
                            for d in m.data {
                                for event in parse_bitget_events(exchange, market, channel, arg_inst, &d, &ctx).await? {
                                    ctx.emit(event).await?;
                                }
                            }
                        }
                        last_seen = Instant::now();
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => last_seen = Instant::now(),
                    Message::Binary(_) | Message::Frame(_) => {}
                    Message::Close(_) => anyhow::bail!("bitget {label} closed"),
                }
            }
        }
    }
}

async fn parse_bitget_events(
    exchange: &'static str,
    market: MarketKind,
    channel: &str,
    arg_inst: Option<&str>,
    data: &Value,
    ctx: &SourceContext,
) -> Result<Vec<DataEvent>> {
    let symbol = first_str(data, &["instId", "symbol"])
        .or(arg_inst)
        .unwrap_or("UNKNOWN");
    let ts_ms = first_str(data, &["ts"])
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or_else(now_ms);

    if channel == "ticker" {
        emit_tick_ext(
            ctx,
            exchange,
            market,
            symbol,
            first_str(data, &["bidPr", "bid", "bidPx"]).unwrap_or("0"),
            first_str(data, &["askPr", "ask", "askPx"]).unwrap_or("0"),
            first_str(data, &["markPrice"]),
            first_str(data, &["fundingRate"]),
            Some(ts_ms),
        )
        .await?;

        let mut events = Vec::new();
        if market == MarketKind::Perp {
            if let Some(funding_rate) =
                first_str(data, &["fundingRate"]).and_then(|x| x.parse::<f64>().ok())
            {
                events.push(DataEvent::FundingRate(FundingRateTick {
                    exchange,
                    symbol: symbol.to_string().into_boxed_str(),
                    funding_rate,
                    next_funding_time_ms: first_str(data, &["nextFundingTime"])
                        .and_then(|x| x.parse::<u64>().ok()),
                    mark_price: first_str(data, &["markPrice"]).and_then(|x| x.parse().ok()),
                    index_price: first_str(data, &["indexPrice"]).and_then(|x| x.parse().ok()),
                    ts_ms,
                }));
            }
            if let Some(open_interest) = first_str(data, &["holdingAmount", "openInterest"])
                .and_then(|x| x.parse::<f64>().ok())
            {
                events.push(DataEvent::OpenInterest(OpenInterestTick {
                    exchange,
                    symbol: symbol.to_string().into_boxed_str(),
                    open_interest,
                    open_interest_value: None,
                    ts_ms,
                }));
            }
        }
        return Ok(events);
    }

    if channel.starts_with("books") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange,
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bids: data
                .get("bids")
                .and_then(Value::as_array)
                .map(|items| parse_levels(items))
                .unwrap_or_default(),
            asks: data
                .get("asks")
                .and_then(Value::as_array)
                .map(|items| parse_levels(items))
                .unwrap_or_default(),
            last_update_id: None,
            ts_ms,
        })]);
    }

    if channel == "trade" {
        return Ok(vec![DataEvent::Trade(TradeTick {
            exchange,
            market,
            symbol: symbol.to_string().into_boxed_str(),
            price: first_str(data, &["price", "px", "p"])
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0),
            qty: first_str(data, &["size", "sz", "qty", "q"])
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0),
            side: side_from_str(first_str(data, &["side", "S"]).unwrap_or_default()),
            trade_id: first_str(data, &["tradeId", "trade_id", "id"])
                .map(|x| x.to_string().into_boxed_str()),
            ts_ms,
        })]);
    }

    Ok(Vec::new())
}

fn first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| match value.get(*key)? {
        Value::String(text) => Some(text.as_str()),
        _ => None,
    })
}

fn parse_levels(items: &[Value]) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            let pair = item.as_array()?;
            Some(BookLevel {
                price: value_to_f64(pair.first()?)?,
                qty: value_to_f64(pair.get(1)?)?,
            })
        })
        .collect()
}

fn value_to_f64(value: &Value) -> Option<f64> {
    value
        .as_str()
        .and_then(|x| x.parse::<f64>().ok())
        .or_else(|| value.as_f64())
}

fn side_from_str(side: &str) -> TradeSide {
    match side.to_ascii_lowercase().as_str() {
        "buy" | "b" | "1" => TradeSide::Buy,
        "sell" | "s" | "2" => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct BitgetSpotTicker {
    pub symbols: Vec<String>,
}
impl BitgetSpotTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[async_trait]
impl ExchangeSource for BitgetSpotTicker {
    fn name(&self) -> &'static str {
        "bitget"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bitget("SPOT", self.name(), MarketKind::Spot, &self.symbols, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_levels, side_from_str};
    use crate::types::TradeSide;
    use serde_json::json;

    #[test]
    fn bitget_side_parser_accepts_common_labels() {
        assert_eq!(side_from_str("buy"), TradeSide::Buy);
        assert_eq!(side_from_str("sell"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }

    #[test]
    fn bitget_level_parser_accepts_strings_and_numbers() {
        let levels = parse_levels(&[json!(["100.5", "2"]), json!([99.5, 3.0])]);

        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].price, 100.5);
        assert_eq!(levels[1].qty, 3.0);
    }
}
