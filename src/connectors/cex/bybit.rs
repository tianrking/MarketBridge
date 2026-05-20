use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{emit_tick_ext, parse_array_levels, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, LiquidationTick, MarketKind, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

#[derive(Deserialize)]
struct BybitMsg {
    #[serde(default)]
    op: Option<String>,
    #[serde(default)]
    ret_msg: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

pub struct BybitSpotTicker {
    pub symbols: Vec<String>,
}

impl BybitSpotTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BybitDepthFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl BybitDepthFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

pub struct BybitTradeFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl BybitTradeFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

pub struct BybitLiquidationFeed {
    symbols: Vec<String>,
}

impl BybitLiquidationFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub async fn run_bybit(
    url: &str,
    exchange: &'static str,
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let label = market_label(market);
    if symbols.is_empty() {
        bail!("bybit {label} symbols empty");
    }

    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();

    let topics = symbols
        .iter()
        .map(|s| format!("tickers.{s}"))
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"op":"subscribe","args":topics}).to_string(),
    ))
    .await?;

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(60) {
                    bail!("bybit {label} pong timeout");
                }
                sink.send(Message::Text(json!({"op":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("bybit {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        if let Ok(m) = serde_json::from_str::<BybitMsg>(&t) {
                            if m.op.as_deref() == Some("pong") || m.ret_msg.as_deref() == Some("pong") {
                                last_pong = Instant::now();
                                continue;
                            }
                            if let Some(d) = m.data {
                                let symbol = d.get("symbol").and_then(|x| x.as_str()).unwrap_or("UNKNOWN");
                                let bid = d.get("bid1Price").and_then(|x| x.as_str()).unwrap_or("0");
                                let ask = d.get("ask1Price").and_then(|x| x.as_str()).unwrap_or("0");
                                let ts = d.get("ts").and_then(|x| x.as_u64());
                                if bid != "0" && ask != "0" {
                                    let mark = (market == MarketKind::Perp)
                                        .then(|| d.get("markPrice").and_then(|x| x.as_str()))
                                        .flatten();
                                    let funding = (market == MarketKind::Perp)
                                        .then(|| d.get("fundingRate").and_then(|x| x.as_str()))
                                        .flatten();
                                    emit_tick_ext(&ctx, exchange, market, symbol, bid, ask, mark, funding, ts).await?;
                                }
                                if market == MarketKind::Perp {
                                    emit_perp_metrics(&ctx, exchange, &d, symbol, ts).await?;
                                }
                            }
                        }
                    }
                    Message::Pong(_) => last_pong = Instant::now(),
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("bybit {label} closed"),
                    Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn emit_perp_metrics(
    ctx: &SourceContext,
    exchange: &'static str,
    d: &serde_json::Value,
    symbol: &str,
    ts: Option<u64>,
) -> Result<()> {
    if let Some(funding_rate) = d
        .get("fundingRate")
        .and_then(|x| x.as_str())
        .and_then(parse_f64)
    {
        ctx.emit(DataEvent::FundingRate(FundingRateTick {
            exchange,
            symbol: symbol.to_string().into_boxed_str(),
            funding_rate,
            next_funding_time_ms: d
                .get("nextFundingTime")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok()),
            mark_price: d
                .get("markPrice")
                .and_then(|x| x.as_str())
                .and_then(parse_f64),
            index_price: d
                .get("indexPrice")
                .and_then(|x| x.as_str())
                .and_then(parse_f64),
            ts_ms: ts.unwrap_or_else(now_ms),
        }))
        .await?;
    }

    if let Some(open_interest) = d
        .get("openInterest")
        .and_then(|x| x.as_str())
        .and_then(parse_f64)
    {
        ctx.emit(DataEvent::OpenInterest(OpenInterestTick {
            exchange,
            symbol: symbol.to_string().into_boxed_str(),
            open_interest,
            open_interest_value: d
                .get("openInterestValue")
                .and_then(|x| x.as_str())
                .and_then(parse_f64),
            ts_ms: ts.unwrap_or_else(now_ms),
        }))
        .await?;
    }
    Ok(())
}

async fn run_bybit_depth(market: MarketKind, symbols: &[String], ctx: SourceContext) -> Result<()> {
    let topics = symbols
        .iter()
        .map(|s| format!("orderbook.50.{s}"))
        .collect::<Vec<_>>();
    run_bybit_topic_loop(bybit_url(market), topics, ctx, move |data| {
        let symbol = data.get("s").and_then(|x| x.as_str())?;
        let bids = data
            .get("b")
            .and_then(|x| x.as_array())
            .map(|x| parse_array_levels(x))
            .unwrap_or_default();
        let asks = data
            .get("a")
            .and_then(|x| x.as_array())
            .map(|x| parse_array_levels(x))
            .unwrap_or_default();
        Some(DataEvent::OrderBook(OrderBookTick {
            exchange: "bybit",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bids,
            asks,
            last_update_id: data.get("u").and_then(|x| x.as_u64()),
            ts_ms: data
                .get("ts")
                .and_then(|x| x.as_u64())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_bybit_trades(
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let topics = symbols
        .iter()
        .map(|s| format!("publicTrade.{s}"))
        .collect::<Vec<_>>();
    run_bybit_topic_loop(bybit_url(market), topics, ctx, move |data| {
        let symbol = data.get("s").and_then(|x| x.as_str())?;
        Some(DataEvent::Trade(TradeTick {
            exchange: "bybit",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            price: data.get("p").and_then(|x| x.as_str()).and_then(parse_f64)?,
            qty: data.get("v").and_then(|x| x.as_str()).and_then(parse_f64)?,
            side: side_from_str(data.get("S").and_then(|x| x.as_str()).unwrap_or_default()),
            trade_id: data
                .get("i")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string().into_boxed_str()),
            ts_ms: data
                .get("T")
                .and_then(|x| x.as_u64())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_bybit_liquidations(symbols: &[String], ctx: SourceContext) -> Result<()> {
    let topics = symbols
        .iter()
        .map(|s| format!("allLiquidation.{s}"))
        .collect::<Vec<_>>();
    run_bybit_topic_loop(
        "wss://stream.bybit.com/v5/public/linear",
        topics,
        ctx,
        |data| {
            let symbol = data.get("s").and_then(|x| x.as_str())?;
            Some(DataEvent::Liquidation(LiquidationTick {
                exchange: "bybit",
                symbol: symbol.to_string().into_boxed_str(),
                side: side_from_str(data.get("S").and_then(|x| x.as_str()).unwrap_or_default()),
                price: data.get("p").and_then(|x| x.as_str()).and_then(parse_f64)?,
                qty: data.get("v").and_then(|x| x.as_str()).and_then(parse_f64)?,
                ts_ms: data
                    .get("T")
                    .and_then(|x| x.as_u64())
                    .unwrap_or_else(now_ms),
            }))
        },
    )
    .await
}

async fn run_bybit_topic_loop<F>(
    url: &str,
    topics: Vec<String>,
    ctx: SourceContext,
    mut build_event: F,
) -> Result<()>
where
    F: FnMut(&serde_json::Value) -> Option<DataEvent>,
{
    if topics.is_empty() {
        bail!("bybit topic list empty");
    }
    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text(
        json!({"op":"subscribe","args":topics}).to_string(),
    ))
    .await?;
    let mut ping_tick = interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Text(json!({"op":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "bybit", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("bybit topic stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(m) = serde_json::from_str::<BybitMsg>(&text)
                            && let Some(data) = m.data
                        {
                            match data {
                                serde_json::Value::Array(items) => {
                                    for item in items {
                                        if let Some(event) = build_event(&item) {
                                            ctx.emit(event).await?;
                                        }
                                    }
                                }
                                item => {
                                    if let Some(event) = build_event(&item) {
                                        ctx.emit(event).await?;
                                    }
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("bybit topic stream closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

fn bybit_url(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "wss://stream.bybit.com/v5/public/spot",
        MarketKind::Perp => "wss://stream.bybit.com/v5/public/linear",
    }
}

fn market_label(market: MarketKind) -> &'static str {
    match market {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["buy"], &["sell"])
}

#[async_trait]
impl ExchangeSource for BybitSpotTicker {
    fn name(&self) -> &'static str {
        "bybit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bybit(
            "wss://stream.bybit.com/v5/public/spot",
            self.name(),
            MarketKind::Spot,
            &self.symbols,
            ctx,
        )
        .await
    }
}

#[async_trait]
impl ExchangeSource for BybitDepthFeed {
    fn name(&self) -> &'static str {
        "bybit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bybit_depth(self.market, &self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BybitTradeFeed {
    fn name(&self) -> &'static str {
        "bybit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bybit_trades(self.market, &self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BybitLiquidationFeed {
    fn name(&self) -> &'static str {
        "bybit"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_bybit_liquidations(&self.symbols, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn bybit_side_parser_accepts_api_labels() {
        assert_eq!(side_from_str("Buy"), TradeSide::Buy);
        assert_eq!(side_from_str("Sell"), TradeSide::Sell);
        assert_eq!(side_from_str("other"), TradeSide::Unknown);
    }
}
