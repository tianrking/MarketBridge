use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::emit_tick_ext;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, LiquidationTick, MarketKind, OpenInterestTick,
    OrderBookTick, TradeSide, TradeTick, now_ms,
};

// ── Shared types ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct SubReq<'a> {
    op: &'a str,
    args: Vec<SubArg<'a>>,
}
#[derive(Serialize)]
struct SubArg<'a> {
    channel: &'a str,
    #[serde(rename = "instId")]
    inst_id: &'a str,
}

#[derive(Deserialize)]
struct Msg<'a> {
    #[serde(default)]
    arg: Option<Arg<'a>>,
    #[serde(default, borrow)]
    data: Vec<Tick<'a>>,
}
#[derive(Deserialize)]
struct Arg<'a> {
    #[serde(borrow, rename = "instId")]
    inst_id: &'a str,
}
#[derive(Deserialize)]
struct Tick<'a> {
    #[serde(borrow, rename = "bidPx")]
    bid: &'a str,
    #[serde(borrow, rename = "askPx")]
    ask: &'a str,
    #[serde(borrow, default, rename = "markPx")]
    mark: Option<&'a str>,
    #[serde(borrow, default, rename = "fundingRate")]
    funding: Option<&'a str>,
    #[serde(borrow, rename = "ts")]
    ts: Option<&'a str>,
}

// ── Shared run loop ───────────────────────────────────────────────────

pub async fn run_okx(
    exchange: &'static str,
    market: MarketKind,
    inst_ids: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let label = if market == MarketKind::Spot {
        "spot"
    } else {
        "perp"
    };
    if inst_ids.is_empty() {
        anyhow::bail!("okx {label} symbols empty");
    }

    let (ws, _) = connect_async("wss://ws.okx.com:8443/ws/v5/public").await?;
    let (mut sink, mut stream) = ws.split();

    let args = inst_ids
        .iter()
        .map(|id| SubArg {
            channel: "tickers",
            inst_id: id.as_str(),
        })
        .collect::<Vec<_>>();
    let sub = serde_json::to_string(&SubReq {
        op: "subscribe",
        args,
    })?;
    sink.send(Message::Text(sub.into())).await?;

    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(60) {
                    anyhow::bail!("okx {label} pong timeout");
                }
                sink.send(Message::Text("ping".into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("okx {label} stream ended"))??;
                match msg {
                    Message::Text(t) => {
                        if t == "pong" { last_pong = Instant::now(); continue; }
                        if let Ok(parsed) = serde_json::from_str::<Msg<'_>>(&t)
                            && let Some(first) = parsed.data.first()
                            && let Some(arg) = parsed.arg
                        {
                            let (mark, funding) = if market == MarketKind::Perp {
                                (first.mark, first.funding)
                            } else {
                                (None, None)
                            };
                            emit_tick_ext(
                                &ctx, exchange, market, arg.inst_id,
                                first.bid, first.ask, mark, funding,
                                first.ts.and_then(|x| x.parse::<u64>().ok()),
                            ).await?;
                        }
                    }
                    Message::Pong(_) => last_pong = Instant::now(),
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => anyhow::bail!("okx {label} closed"),
                    Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

// ── Spot ──────────────────────────────────────────────────────────────

pub struct OkxTicker {
    pub inst_ids: Vec<String>,
}

pub struct OkxFundingFeed {
    inst_ids: Vec<String>,
}

impl OkxFundingFeed {
    pub fn new(inst_ids: Vec<String>) -> Self {
        Self { inst_ids }
    }
}

pub struct OkxOpenInterestFeed {
    inst_ids: Vec<String>,
}

impl OkxOpenInterestFeed {
    pub fn new(inst_ids: Vec<String>) -> Self {
        Self { inst_ids }
    }
}

pub struct OkxDepthFeed {
    market: MarketKind,
    inst_ids: Vec<String>,
}

impl OkxDepthFeed {
    pub fn new(market: MarketKind, inst_ids: Vec<String>) -> Self {
        Self { market, inst_ids }
    }
}

pub struct OkxTradeFeed {
    market: MarketKind,
    inst_ids: Vec<String>,
}

impl OkxTradeFeed {
    pub fn new(market: MarketKind, inst_ids: Vec<String>) -> Self {
        Self { market, inst_ids }
    }
}

pub struct OkxLiquidationPoller {
    inst_ids: Vec<String>,
}

impl OkxLiquidationPoller {
    pub fn new(inst_ids: Vec<String>) -> Self {
        Self { inst_ids }
    }
}
impl OkxTicker {
    pub fn new(inst_ids: Vec<String>) -> Self {
        Self { inst_ids }
    }
}

#[async_trait]
impl ExchangeSource for OkxTicker {
    fn name(&self) -> &'static str {
        "okx"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx(self.name(), MarketKind::Spot, &self.inst_ids, ctx).await
    }
}

async fn run_okx_channel<F>(
    channel: &'static str,
    inst_ids: &[String],
    ctx: SourceContext,
    mut build_event: F,
) -> Result<()>
where
    F: FnMut(&str, &serde_json::Value) -> Option<DataEvent>,
{
    if inst_ids.is_empty() {
        bail!("okx {channel} instruments empty");
    }
    let (ws, _) = connect_async("wss://ws.okx.com:8443/ws/v5/public").await?;
    let (mut sink, mut stream) = ws.split();
    let args = inst_ids
        .iter()
        .map(|id| serde_json::json!({"channel": channel, "instId": id}))
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        serde_json::json!({"op":"subscribe","args":args})
            .to_string()
            .into(),
    ))
    .await?;
    let mut ping_tick = interval(Duration::from_secs(20));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(60) {
                    bail!("okx {channel} pong timeout");
                }
                sink.send(Message::Text("ping".into())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "okx", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("okx {channel} stream ended"))??;
                match msg {
                    Message::Text(text) => {
                        if text == "pong" {
                            last_pong = Instant::now();
                            continue;
                        }
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                            let inst_id = value
                                .get("arg")
                                .and_then(|x| x.get("instId"))
                                .and_then(|x| x.as_str())
                                .unwrap_or("UNKNOWN");
                            if let Some(items) = value.get("data").and_then(|x| x.as_array()) {
                                for item in items {
                                    if let Some(event) = build_event(inst_id, item) {
                                        ctx.emit(event).await?;
                                    }
                                }
                            }
                        }
                    }
                    Message::Pong(_) => last_pong = Instant::now(),
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("okx {channel} closed"),
                    Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_okx_funding(inst_ids: &[String], ctx: SourceContext) -> Result<()> {
    run_okx_channel("funding-rate", inst_ids, ctx, |inst_id, item| {
        Some(DataEvent::FundingRate(FundingRateTick {
            exchange: "okx",
            symbol: inst_id.to_string().into_boxed_str(),
            funding_rate: item
                .get("fundingRate")
                .and_then(|x| x.as_str())
                .and_then(parse_f64)?,
            next_funding_time_ms: item
                .get("nextFundingTime")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok()),
            mark_price: item
                .get("markPx")
                .and_then(|x| x.as_str())
                .and_then(parse_f64),
            index_price: None,
            ts_ms: item
                .get("ts")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_okx_open_interest(inst_ids: &[String], ctx: SourceContext) -> Result<()> {
    run_okx_channel("open-interest", inst_ids, ctx, |inst_id, item| {
        Some(DataEvent::OpenInterest(OpenInterestTick {
            exchange: "okx",
            symbol: inst_id.to_string().into_boxed_str(),
            open_interest: item
                .get("oi")
                .and_then(|x| x.as_str())
                .and_then(parse_f64)?,
            open_interest_value: item
                .get("oiUsd")
                .and_then(|x| x.as_str())
                .and_then(parse_f64),
            ts_ms: item
                .get("ts")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_okx_depth(market: MarketKind, inst_ids: &[String], ctx: SourceContext) -> Result<()> {
    run_okx_channel("books5", inst_ids, ctx, move |inst_id, item| {
        Some(DataEvent::OrderBook(OrderBookTick {
            exchange: "okx",
            market,
            symbol: inst_id.to_string().into_boxed_str(),
            bids: item
                .get("bids")
                .and_then(|x| x.as_array())
                .map(|x| parse_okx_levels(x))
                .unwrap_or_default(),
            asks: item
                .get("asks")
                .and_then(|x| x.as_array())
                .map(|x| parse_okx_levels(x))
                .unwrap_or_default(),
            last_update_id: None,
            ts_ms: item
                .get("ts")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_okx_trades(market: MarketKind, inst_ids: &[String], ctx: SourceContext) -> Result<()> {
    run_okx_channel("trades", inst_ids, ctx, move |inst_id, item| {
        Some(DataEvent::Trade(TradeTick {
            exchange: "okx",
            market,
            symbol: inst_id.to_string().into_boxed_str(),
            price: item
                .get("px")
                .and_then(|x| x.as_str())
                .and_then(parse_f64)?,
            qty: item
                .get("sz")
                .and_then(|x| x.as_str())
                .and_then(parse_f64)?,
            side: side_from_str(
                item.get("side")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default(),
            ),
            trade_id: item
                .get("tradeId")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string().into_boxed_str()),
            ts_ms: item
                .get("ts")
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or_else(now_ms),
        }))
    })
    .await
}

async fn run_okx_liquidations(inst_ids: &[String], ctx: SourceContext) -> Result<()> {
    if inst_ids.is_empty() {
        bail!("okx liquidation instruments empty");
    }
    let client = reqwest::Client::new();
    let mut poll = interval(Duration::from_secs(10));
    loop {
        poll.tick().await;
        for inst_id in inst_ids {
            let response = client
                .get("https://www.okx.com/api/v5/public/liquidation-orders")
                .query(&[("instType", "SWAP"), ("instId", inst_id.as_str())])
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await?;
            if let Some(items) = response.get("data").and_then(|x| x.as_array()) {
                for item in items {
                    if let Some(details) = item.get("details").and_then(|x| x.as_array()) {
                        for detail in details {
                            let Some(price) = detail
                                .get("bkPx")
                                .and_then(|x| x.as_str())
                                .and_then(parse_f64)
                            else {
                                continue;
                            };
                            let Some(qty) = detail
                                .get("sz")
                                .and_then(|x| x.as_str())
                                .and_then(parse_f64)
                            else {
                                continue;
                            };
                            ctx.emit(DataEvent::Liquidation(LiquidationTick {
                                exchange: "okx",
                                symbol: inst_id.clone().into_boxed_str(),
                                side: side_from_str(
                                    item.get("side")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or_default(),
                                ),
                                price,
                                qty,
                                ts_ms: detail
                                    .get("ts")
                                    .and_then(|x| x.as_str())
                                    .and_then(|x| x.parse::<u64>().ok())
                                    .unwrap_or_else(now_ms),
                            }))
                            .await?;
                        }
                    }
                }
            }
        }
    }
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn parse_okx_levels(items: &[serde_json::Value]) -> Vec<BookLevel> {
    items
        .iter()
        .filter_map(|item| {
            let pair = item.as_array()?;
            Some(BookLevel {
                price: pair.first()?.as_str()?.parse::<f64>().ok()?,
                qty: pair.get(1)?.as_str()?.parse::<f64>().ok()?,
            })
        })
        .collect()
}

fn side_from_str(side: &str) -> TradeSide {
    match side {
        "buy" | "BUY" => TradeSide::Buy,
        "sell" | "SELL" => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

#[async_trait]
impl ExchangeSource for OkxFundingFeed {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx_funding(&self.inst_ids, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for OkxOpenInterestFeed {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx_open_interest(&self.inst_ids, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for OkxDepthFeed {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx_depth(self.market, &self.inst_ids, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for OkxTradeFeed {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx_trades(self.market, &self.inst_ids, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for OkxLiquidationPoller {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_okx_liquidations(&self.inst_ids, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::side_from_str;
    use crate::types::TradeSide;

    #[test]
    fn okx_side_parser_accepts_api_labels() {
        assert_eq!(side_from_str("buy"), TradeSide::Buy);
        assert_eq!(side_from_str("sell"), TradeSide::Sell);
        assert_eq!(side_from_str("other"), TradeSide::Unknown);
    }
}
