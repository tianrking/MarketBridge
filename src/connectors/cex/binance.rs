use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt, future::join_all};
use serde::Deserialize;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{emit_tick, emit_tick_ext};
use crate::connectors::cex::ws::message_text;
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
    stream: &'a str,
    #[serde(borrow)]
    data: BinanceDepthMsg<'a>,
}

#[derive(Debug, Deserialize)]
struct BinanceDepthMsg<'a> {
    #[serde(borrow, default, rename = "s")]
    symbol: Option<&'a str>,
    #[serde(default, rename = "E")]
    event_time_ms: Option<u64>,
    #[serde(rename = "lastUpdateId", alias = "u")]
    last_update_id: Option<u64>,
    #[serde(borrow, default, rename = "bids", alias = "b")]
    bids: Vec<[&'a str; 2]>,
    #[serde(borrow, default, rename = "asks", alias = "a")]
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
                    Message::Text(_) | Message::Binary(_) => {
                        let Some(text) = decode_ws_text("binance bookTicker", &msg)? else {
                            continue;
                        };
                        if let Some(parsed) = parse_ws_json::<BinanceCombined<'_>>("binance bookTicker", &text) {
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
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_funding(symbols: &[String], ctx: SourceContext) -> Result<()> {
    let streams = combined_streams(symbols, "markPrice@1s")?;
    let ws_url = format!("wss://fstream.binance.com/market/stream?streams={streams}");
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
                    Message::Text(_) | Message::Binary(_) => {
                        let Some(text) = decode_ws_text("binance funding", &msg)? else {
                            continue;
                        };
                        if let Some(parsed) = parse_ws_json::<BinanceFundingCombined<'_>>("binance funding", &text)
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
                    Message::Pong(_) | Message::Frame(_) => {}
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
    let allowed = symbols
        .iter()
        .map(|s| s.to_ascii_uppercase())
        .collect::<std::collections::HashSet<_>>();
    let streams = combined_streams(symbols, suffix)?;
    let base = match market {
        MarketKind::Spot => "wss://stream.binance.com:9443/stream?",
        MarketKind::Perp => "wss://fstream.binance.com/public/stream?",
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
                    Message::Text(_) | Message::Binary(_) => {
                        let Some(text) = decode_ws_text("binance depth", &msg)? else {
                            continue;
                        };
                        if let Some(book) = parse_ws_json::<BinanceDepthCombined<'_>>("binance depth", &text)
                            .and_then(|parsed| depth_book(parsed, market, now_ms()))
                            .filter(|book| allowed.contains(book.symbol.as_ref())) {
                            ctx.emit(DataEvent::OrderBook(book)).await?;
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("binance depth closed"),
                    Message::Pong(_) | Message::Frame(_) => {}
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
        MarketKind::Perp => "wss://fstream.binance.com/market/stream?",
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
                    Message::Text(_) | Message::Binary(_) => {
                        let Some(text) = decode_ws_text("binance aggTrade", &msg)? else {
                            continue;
                        };
                        if let Some(parsed) = parse_ws_json::<BinanceAggTradeCombined<'_>>("binance aggTrade", &text)
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
                    Message::Pong(_) | Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn run_binance_liquidations(symbols: &[String], ctx: SourceContext) -> Result<()> {
    let streams = combined_streams(symbols, "forceOrder")?;
    let ws_url = format!("wss://fstream.binance.com/market/stream?streams={streams}");
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
                    Message::Text(_) | Message::Binary(_) => {
                        let Some(text) = decode_ws_text("binance liquidation", &msg)? else {
                            continue;
                        };
                        if let Some(parsed) = parse_ws_json::<BinanceLiquidationCombined<'_>>("binance liquidation", &text)
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
                    Message::Pong(_) | Message::Frame(_) => {}
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
        let results = join_all(
            symbols
                .iter()
                .map(|symbol| fetch_binance_open_interest(&client, symbol)),
        )
        .await;
        for result in results {
            match result {
                Ok(Some(event)) => ctx.emit(event).await?,
                Ok(None) => {}
                Err(error) => warn!(%error, "binance open interest refresh failed"),
            }
        }
    }
}

async fn fetch_binance_open_interest(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<Option<DataEvent>> {
    let response = client
        .get("https://fapi.binance.com/fapi/v1/openInterest")
        .query(&[("symbol", symbol)])
        .send()
        .await?
        .error_for_status()?
        .json::<BinanceOpenInterestResponse>()
        .await?;
    Ok(binance_open_interest_event(response))
}

fn binance_open_interest_event(response: BinanceOpenInterestResponse) -> Option<DataEvent> {
    let Ok(open_interest) = response.open_interest.parse::<f64>() else {
        warn!(
            symbol = response.symbol,
            value = response.open_interest,
            "binance open interest parse failed"
        );
        return None;
    };
    Some(DataEvent::OpenInterest(OpenInterestTick {
        exchange: "binance",
        symbol: response.symbol.into_boxed_str(),
        open_interest,
        open_interest_value: None,
        ts_ms: response.time.unwrap_or_else(now_ms),
    }))
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

fn decode_ws_text(label: &'static str, msg: &Message) -> Result<Option<String>> {
    match message_text(msg) {
        Ok(text) => Ok(text),
        Err(error) => {
            warn!(label, %error, "websocket message decode failed");
            Ok(None)
        }
    }
}

fn parse_ws_json<'a, T>(label: &'static str, text: &'a str) -> Option<T>
where
    T: Deserialize<'a>,
{
    match serde_json::from_str::<T>(text) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            warn!(
                label,
                %error,
                bytes = text.len(),
                "websocket json parse failed"
            );
            None
        }
    }
}

fn parse_f64(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}

fn parse_levels(levels: &[[&str; 2]]) -> Option<Vec<BookLevel>> {
    if levels.is_empty() || levels.len() > 20 {
        return None;
    }
    levels
        .iter()
        .map(|[price, qty]| {
            let price = price.parse::<f64>().ok()?;
            let qty = qty.parse::<f64>().ok()?;
            (price.is_finite() && price > 0.0 && qty.is_finite() && qty > 0.0)
                .then_some(BookLevel { price, qty })
        })
        .collect()
}

fn depth_book(
    parsed: BinanceDepthCombined<'_>,
    market: MarketKind,
    received_at_ms: u64,
) -> Option<OrderBookTick> {
    // Spot partial-depth payloads omit `s`; identity is in the combined stream.
    let symbol = parsed
        .stream
        .strip_suffix("@depth20@100ms")?
        .to_ascii_uppercase();
    if symbol.is_empty()
        || parsed
            .data
            .symbol
            .is_some_and(|s| !s.eq_ignore_ascii_case(&symbol))
    {
        return None;
    }
    let bids = parse_levels(&parsed.data.bids)?;
    let asks = parse_levels(&parsed.data.asks)?;
    if bids[0].price > asks[0].price
        || !bids.windows(2).all(|w| w[0].price > w[1].price)
        || !asks.windows(2).all(|w| w[0].price < w[1].price)
    {
        return None;
    }
    Some(OrderBookTick {
        exchange: "binance",
        market,
        symbol: symbol.into(),
        bids,
        asks,
        last_update_id: Some(parsed.data.last_update_id?),
        ts_ms: parsed.data.event_time_ms.unwrap_or(received_at_ms),
    })
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
    fn source_type(&self) -> &'static str {
        "book_ticker"
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
    fn source_type(&self) -> &'static str {
        "funding"
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
    fn source_type(&self) -> &'static str {
        "open_interest"
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
    fn source_type(&self) -> &'static str {
        "liquidation"
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
    fn source_type(&self) -> &'static str {
        "depth"
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
    fn source_type(&self) -> &'static str {
        "trade"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        run_binance_trades(self.market, &self.symbols, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BinanceDepthCombined, BinanceOpenInterestResponse, binance_open_interest_event, depth_book,
        side_from_str, trade_side_from_buyer_maker,
    };
    use crate::types::{DataEvent, MarketKind, TradeSide};

    #[test]
    fn binance_side_helpers_map_exchange_semantics() {
        assert_eq!(trade_side_from_buyer_maker(Some(true)), TradeSide::Sell);
        assert_eq!(trade_side_from_buyer_maker(Some(false)), TradeSide::Buy);
        assert_eq!(trade_side_from_buyer_maker(None), TradeSide::Unknown);
        assert_eq!(side_from_str("BUY"), TradeSide::Buy);
        assert_eq!(side_from_str("SELL"), TradeSide::Sell);
    }

    #[test]
    fn binance_open_interest_response_maps_to_event() {
        let event = binance_open_interest_event(BinanceOpenInterestResponse {
            symbol: "BTCUSDT".to_string(),
            open_interest: "123.45".to_string(),
            time: Some(42),
        })
        .expect("event");

        let DataEvent::OpenInterest(tick) = event else {
            panic!("expected open interest");
        };
        assert_eq!(tick.exchange, "binance");
        assert_eq!(tick.symbol.as_ref(), "BTCUSDT");
        assert_eq!(tick.open_interest, 123.45);
        assert_eq!(tick.ts_ms, 42);
    }

    #[test]
    fn binance_depth_accepts_futures_short_book_fields() {
        let parsed: BinanceDepthCombined<'_> = serde_json::from_str(
            r#"{
                "stream":"homeusdt@depth20@100ms",
                "data":{
                    "e":"depthUpdate",
                    "E":1780642750000,
                    "s":"HOMEUSDT",
                    "u":390497878,
                    "b":[["0.04850","1000"]],
                    "a":[["0.04860","2000"]]
                }
            }"#,
        )
        .expect("depth");

        assert_eq!(parsed.data.symbol, Some("HOMEUSDT"));
        assert_eq!(parsed.data.last_update_id, Some(390497878));
        assert_eq!(parsed.data.bids[0], ["0.04850", "1000"]);
        assert_eq!(parsed.data.asks[0], ["0.04860", "2000"]);
        let book = depth_book(parsed, MarketKind::Perp, 99).unwrap();
        assert_eq!(book.ts_ms, 1780642750000);
    }

    #[test]
    fn spot_partial_depth_uses_stream_identity_without_payload_symbol() {
        let parsed=serde_json::from_str(r#"{"stream":"btcusdt@depth20@100ms","data":{"lastUpdateId":123,"bids":[["99","1"]],"asks":[["100","2"]]}}"#).unwrap();
        let book = depth_book(parsed, MarketKind::Spot, 42).unwrap();
        assert_eq!(book.symbol.as_ref(), "BTCUSDT");
        assert_eq!(book.ts_ms, 42);
        assert_eq!(book.last_update_id, Some(123));
    }

    #[test]
    fn depth_identity_conflict_or_invalid_levels_cannot_create_complete_book() {
        for data in [
            r#"{"stream":"btcusdt@depth20@100ms","data":{"s":"ETHUSDT","u":1,"bids":[["99","1"]],"asks":[["100","2"]]}}"#,
            r#"{"stream":"btcusdt@depth20@100ms","data":{"u":1,"bids":[["NaN","1"]],"asks":[["100","2"]]}}"#,
            r#"{"stream":"btcusdt@depth@100ms","data":{"u":1,"bids":[["99","1"]],"asks":[["100","2"]]}}"#,
        ] {
            assert!(
                depth_book(serde_json::from_str(data).unwrap(), MarketKind::Spot, 42).is_none()
            );
        }
    }
}
