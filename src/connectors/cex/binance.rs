use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{emit_tick, emit_tick_ext};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    BookLevel, DataEvent, FundingRateTick, LiquidationTick, MarketKind, OpenInterestTick,
    OrderBookTick, TradeSide, TradeTick, now_ms,
};

// ── Spot ──────────────────────────────────────────────────────────────

pub struct BinanceBookTicker {
    symbols: Vec<String>,
}

impl BinanceBookTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BinanceFundingTicker {
    symbols: Vec<String>,
}

impl BinanceFundingTicker {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BinanceOpenInterestPoller {
    symbols: Vec<String>,
}

impl BinanceOpenInterestPoller {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BinanceLiquidationFeed {
    symbols: Vec<String>,
}

impl BinanceLiquidationFeed {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

pub struct BinanceDepthFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl BinanceDepthFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

pub struct BinanceTradeFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl BinanceTradeFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

#[derive(Debug, Deserialize)]
struct BinanceCombined<'a> {
    #[serde(borrow)]
    data: BinanceBookTickerMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceBookTickerMsg<'a> {
    #[serde(borrow, rename = "s")]
    symbol: &'a str,
    #[serde(borrow, rename = "b")]
    bid: &'a str,
    #[serde(borrow, rename = "a")]
    ask: &'a str,
}

#[derive(Debug, Deserialize)]
struct BinanceFundingCombined<'a> {
    #[serde(borrow)]
    data: BinanceFundingMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceFundingMsg<'a> {
    #[serde(borrow, rename = "s")]
    symbol: &'a str,
    #[serde(borrow, rename = "p")]
    mark_price: Option<&'a str>,
    #[serde(borrow, rename = "i")]
    index_price: Option<&'a str>,
    #[serde(borrow, rename = "r")]
    funding_rate: &'a str,
    #[serde(rename = "T")]
    next_funding_time_ms: Option<u64>,
    #[serde(rename = "E")]
    event_time_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BinanceDepthCombined<'a> {
    #[serde(borrow)]
    data: BinanceDepthMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceDepthMsg<'a> {
    #[serde(borrow, rename = "s")]
    symbol: &'a str,
    #[serde(rename = "lastUpdateId")]
    last_update_id: Option<u64>,
    #[serde(borrow, default)]
    bids: Vec<[&'a str; 2]>,
    #[serde(borrow, default)]
    asks: Vec<[&'a str; 2]>,
}

#[derive(Debug, Deserialize)]
struct BinanceAggTradeCombined<'a> {
    #[serde(borrow)]
    data: BinanceAggTradeMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceAggTradeMsg<'a> {
    #[serde(borrow, rename = "s")]
    symbol: &'a str,
    #[serde(rename = "a")]
    trade_id: Option<u64>,
    #[serde(borrow, rename = "p")]
    price: &'a str,
    #[serde(borrow, rename = "q")]
    qty: &'a str,
    #[serde(rename = "m")]
    buyer_is_maker: Option<bool>,
    #[serde(rename = "T")]
    trade_time_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BinanceLiquidationCombined<'a> {
    #[serde(borrow)]
    data: BinanceLiquidationMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceLiquidationMsg<'a> {
    #[serde(borrow, rename = "o")]
    order: BinanceLiquidationOrder<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceLiquidationOrder<'a> {
    #[serde(borrow, rename = "s")]
    symbol: &'a str,
    #[serde(borrow, rename = "S")]
    side: &'a str,
    #[serde(borrow, rename = "p")]
    price: &'a str,
    #[serde(borrow, rename = "q")]
    qty: &'a str,
    #[serde(rename = "T")]
    trade_time_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BinanceOpenInterestResponse {
    symbol: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    time: Option<u64>,
}

pub async fn run_binance(
    url: &str,
    exchange: &'static str,
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    if symbols.is_empty() {
        anyhow::bail!(
            "binance {} symbols empty",
            if market == MarketKind::Spot {
                "spot"
            } else {
                "perp"
            }
        );
    }

    let streams = symbols
        .iter()
        .map(|s| format!("{}@bookTicker", s.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join("/");
    let ws_url = format!("{url}streams={streams}");

    let (ws, _) = connect_async(&ws_url)
        .await
        .with_context(|| format!("binance {} connect failed", market_label(market)))?;
    let (mut sink, mut stream) = ws.split();
    let mut ping_tick = interval(Duration::from_secs(15));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(60) {
                    anyhow::bail!("binance {} pong timeout", market_label(market));
                }
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context(format!("binance {} stream ended", market_label(market)))??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(parsed) = serde_json::from_str::<BinanceCombined<'_>>(&text) {
                            match market {
                                MarketKind::Spot => {
                                    emit_tick(&ctx, exchange, market, parsed.data.symbol, parsed.data.bid, parsed.data.ask).await?;
                                }
                                MarketKind::Perp => {
                                    // Perp stream includes event timestamp; spot does not
                                    emit_tick_ext(&ctx, exchange, market, parsed.data.symbol, parsed.data.bid, parsed.data.ask, None, None, None).await?;
                                }
                            }
                        }
                    }
                    Message::Pong(_) => last_pong = Instant::now(),
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => anyhow::bail!("binance {} closed", market_label(market)),
                    Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_funding(symbols: &[String], ctx: SourceContext) -> Result<()> {
    let streams = combined_streams(symbols, "markPrice@1s")?;
    let ws_url = format!("wss://fstream.binance.com/stream?streams={streams}");
    let (ws, _) = connect_async(&ws_url)
        .await
        .context("binance funding connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let mut ping_tick = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "binance", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("binance funding stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(parsed) = serde_json::from_str::<BinanceFundingCombined<'_>>(&text)
                            && let Ok(funding_rate) = parsed.data.funding_rate.parse::<f64>()
                        {
                            ctx.emit(DataEvent::FundingRate(FundingRateTick {
                                exchange: "binance",
                                symbol: parsed.data.symbol.to_string().into_boxed_str(),
                                funding_rate,
                                next_funding_time_ms: parsed.data.next_funding_time_ms,
                                mark_price: parsed.data.mark_price.and_then(parse_f64),
                                index_price: parsed.data.index_price.and_then(parse_f64),
                                ts_ms: parsed.data.event_time_ms.unwrap_or_else(now_ms),
                            })).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("binance funding closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_depth(
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let suffix = "depth20@100ms";
    let streams = combined_streams(symbols, suffix)?;
    let base = match market {
        MarketKind::Spot => "wss://stream.binance.com:9443/stream?",
        MarketKind::Perp => "wss://fstream.binance.com/stream?",
    };
    let ws_url = format!("{base}streams={streams}");
    let (ws, _) = connect_async(&ws_url)
        .await
        .context("binance depth connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let mut ping_tick = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "binance", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("binance depth stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(parsed) = serde_json::from_str::<BinanceDepthCombined<'_>>(&text) {
                            ctx.emit(DataEvent::OrderBook(OrderBookTick {
                                exchange: "binance",
                                market,
                                symbol: parsed.data.symbol.to_string().into_boxed_str(),
                                bids: parse_levels(&parsed.data.bids),
                                asks: parse_levels(&parsed.data.asks),
                                last_update_id: parsed.data.last_update_id,
                                ts_ms: now_ms(),
                            })).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("binance depth closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_trades(
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    let streams = combined_streams(symbols, "aggTrade")?;
    let base = match market {
        MarketKind::Spot => "wss://stream.binance.com:9443/stream?",
        MarketKind::Perp => "wss://fstream.binance.com/stream?",
    };
    let ws_url = format!("{base}streams={streams}");
    let (ws, _) = connect_async(&ws_url)
        .await
        .context("binance trades connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let mut ping_tick = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "binance", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("binance trade stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(parsed) = serde_json::from_str::<BinanceAggTradeCombined<'_>>(&text)
                            && let (Ok(price), Ok(qty)) = (parsed.data.price.parse::<f64>(), parsed.data.qty.parse::<f64>())
                        {
                            ctx.emit(DataEvent::Trade(TradeTick {
                                exchange: "binance",
                                market,
                                symbol: parsed.data.symbol.to_string().into_boxed_str(),
                                price,
                                qty,
                                side: trade_side_from_buyer_maker(parsed.data.buyer_is_maker),
                                trade_id: parsed.data.trade_id.map(|id| id.to_string().into_boxed_str()),
                                ts_ms: parsed.data.trade_time_ms.unwrap_or_else(now_ms),
                            })).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("binance trades closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_liquidations(symbols: &[String], ctx: SourceContext) -> Result<()> {
    let streams = combined_streams(symbols, "forceOrder")?;
    let ws_url = format!("wss://fstream.binance.com/stream?streams={streams}");
    let (ws, _) = connect_async(&ws_url)
        .await
        .context("binance liquidation connect failed")?;
    let (mut sink, mut stream) = ws.split();
    let mut ping_tick = interval(Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                sink.send(Message::Ping(Vec::new())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "binance", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("binance liquidation stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(parsed) = serde_json::from_str::<BinanceLiquidationCombined<'_>>(&text)
                            && let (Ok(price), Ok(qty)) = (parsed.data.order.price.parse::<f64>(), parsed.data.order.qty.parse::<f64>())
                        {
                            ctx.emit(DataEvent::Liquidation(LiquidationTick {
                                exchange: "binance",
                                symbol: parsed.data.order.symbol.to_string().into_boxed_str(),
                                side: side_from_str(parsed.data.order.side),
                                price,
                                qty,
                                ts_ms: parsed.data.order.trade_time_ms.unwrap_or_else(now_ms),
                            })).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("binance liquidation closed"),
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_open_interest(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("binance open interest symbols empty");
    }
    let client = reqwest::Client::new();
    let mut tick = interval(Duration::from_secs(5));
    loop {
        tick.tick().await;
        for symbol in symbols {
            let response = client
                .get("https://fapi.binance.com/fapi/v1/openInterest")
                .query(&[("symbol", symbol)])
                .send()
                .await?
                .error_for_status()?
                .json::<BinanceOpenInterestResponse>()
                .await?;
            if let Ok(open_interest) = response.open_interest.parse::<f64>() {
                ctx.emit(DataEvent::OpenInterest(OpenInterestTick {
                    exchange: "binance",
                    symbol: response.symbol.into_boxed_str(),
                    open_interest,
                    open_interest_value: None,
                    ts_ms: response.time.unwrap_or_else(now_ms),
                }))
                .await?;
            }
        }
    }
}

fn combined_streams(symbols: &[String], suffix: &str) -> Result<String> {
    if symbols.is_empty() {
        bail!("binance {suffix} symbols empty");
    }
    Ok(symbols
        .iter()
        .map(|s| format!("{}@{suffix}", s.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join("/"))
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn parse_levels(levels: &[[&str; 2]]) -> Vec<BookLevel> {
    levels
        .iter()
        .filter_map(|[price, qty]| {
            Some(BookLevel {
                price: price.parse::<f64>().ok()?,
                qty: qty.parse::<f64>().ok()?,
            })
        })
        .collect()
}

fn trade_side_from_buyer_maker(buyer_is_maker: Option<bool>) -> TradeSide {
    match buyer_is_maker {
        Some(true) => TradeSide::Sell,
        Some(false) => TradeSide::Buy,
        None => TradeSide::Unknown,
    }
}

fn side_from_str(side: &str) -> TradeSide {
    match side {
        "BUY" => TradeSide::Buy,
        "SELL" => TradeSide::Sell,
        _ => TradeSide::Unknown,
    }
}

fn market_label(m: MarketKind) -> &'static str {
    match m {
        MarketKind::Spot => "spot",
        MarketKind::Perp => "perp",
    }
}

#[async_trait]
impl ExchangeSource for BinanceBookTicker {
    fn name(&self) -> &'static str {
        "binance"
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance(
            "wss://stream.binance.com:9443/stream?",
            self.name(),
            MarketKind::Spot,
            &self.symbols,
            ctx,
        )
        .await
    }
}

#[async_trait]
impl ExchangeSource for BinanceFundingTicker {
    fn name(&self) -> &'static str {
        "binance"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_funding(&self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BinanceOpenInterestPoller {
    fn name(&self) -> &'static str {
        "binance"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_open_interest(&self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BinanceLiquidationFeed {
    fn name(&self) -> &'static str {
        "binance"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_liquidations(&self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BinanceDepthFeed {
    fn name(&self) -> &'static str {
        "binance"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_depth(self.market, &self.symbols, ctx).await
    }
}

#[async_trait]
impl ExchangeSource for BinanceTradeFeed {
    fn name(&self) -> &'static str {
        "binance"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_trades(self.market, &self.symbols, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::{side_from_str, trade_side_from_buyer_maker};
    use crate::types::TradeSide;

    #[test]
    fn binance_side_helpers_map_exchange_semantics() {
        assert_eq!(trade_side_from_buyer_maker(Some(true)), TradeSide::Sell);
        assert_eq!(trade_side_from_buyer_maker(Some(false)), TradeSide::Buy);
        assert_eq!(trade_side_from_buyer_maker(None), TradeSide::Unknown);
        assert_eq!(side_from_str("BUY"), TradeSide::Buy);
        assert_eq!(side_from_str("SELL"), TradeSide::Sell);
    }
}
