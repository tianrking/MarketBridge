use async_trait::async_trait;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::connectors::cex::common::{emit_tick_ext, side_from_labels};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, LiquidationTick, MarketKind, OpenInterestTick, OrderBookTick,
    TradeSide, TradeTick, now_ms,
};

#[derive(Deserialize)]
struct BybitMsg {
    #[serde(default, rename = "type")]
    message_type: Option<String>,
    #[serde(default)]
    ts: Option<serde_json::Value>,
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
                                let ts = m.ts.as_ref().and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
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

#[derive(Default)]
struct BybitBook {
    bids: Vec<crate::types::BookLevel>,
    asks: Vec<crate::types::BookLevel>,
    update_id: Option<u64>,
}

impl BybitBook {
    fn apply(
        &mut self,
        data: &serde_json::Value,
        kind: Option<&str>,
        ts: Option<u64>,
        market: MarketKind,
    ) -> Option<OrderBookTick> {
        let id = data.get("u")?.as_u64()?;
        let snapshot = kind == Some("snapshot") || id == 1;
        if !snapshot && (kind != Some("delta") || self.update_id.is_none()) {
            return None;
        }
        if !snapshot && self.update_id.is_some_and(|last| id <= last) {
            return None;
        }
        let changes = parse_book_changes(data);
        let Some((bids, asks)) = changes else {
            *self = Self::default();
            return None;
        };
        if snapshot {
            self.bids.clear();
            self.asks.clear();
        }
        apply_book_changes(&mut self.bids, bids, false);
        apply_book_changes(&mut self.asks, asks, true);
        if self.bids.is_empty() || self.asks.is_empty() || self.bids[0].price > self.asks[0].price {
            *self = Self::default();
            return None;
        }
        self.update_id = Some(id);
        Some(OrderBookTick {
            exchange: "bybit",
            market,
            symbol: data.get("s")?.as_str()?.into(),
            bids: self.bids.clone(),
            asks: self.asks.clone(),
            last_update_id: Some(id),
            ts_ms: ts.unwrap_or_else(now_ms),
        })
    }
}

fn parse_book_changes(
    data: &serde_json::Value,
) -> Option<(Vec<crate::types::BookLevel>, Vec<crate::types::BookLevel>)> {
    fn side(data: &serde_json::Value) -> Option<Vec<crate::types::BookLevel>> {
        let rows = data.as_array()?;
        if rows.len() > 1000 {
            return None;
        }
        rows.iter()
            .map(|row| {
                let row = row.as_array()?;
                let number = |v: &serde_json::Value| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                };
                let price = number(row.first()?)?;
                let qty = number(row.get(1)?)?;
                (price.is_finite() && price > 0.0 && qty.is_finite() && qty >= 0.0)
                    .then_some(crate::types::BookLevel { price, qty })
            })
            .collect()
    }
    Some((side(data.get("b")?)?, side(data.get("a")?)?))
}

fn apply_book_changes(
    levels: &mut Vec<crate::types::BookLevel>,
    changes: Vec<crate::types::BookLevel>,
    ascending: bool,
) {
    for change in changes {
        levels.retain(|l| l.price != change.price);
        if change.qty > 0.0 {
            levels.push(change);
        }
    }
    levels.sort_by(|a, b| {
        if ascending {
            a.price.total_cmp(&b.price)
        } else {
            b.price.total_cmp(&a.price)
        }
    });
    levels.truncate(50);
}

async fn run_bybit_depth(market: MarketKind, symbols: &[String], ctx: SourceContext) -> Result<()> {
    let topics = symbols
        .iter()
        .map(|s| format!("orderbook.50.{s}"))
        .collect::<Vec<_>>();
    let allowed = symbols
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut books = std::collections::HashMap::<String, BybitBook>::new();
    run_bybit_topic_loop(bybit_url(market), topics, ctx, move |data, kind, ts| {
        let symbol = data.get("s").and_then(|x| x.as_str())?;
        if !allowed.contains(symbol) {
            return None;
        }
        books
            .entry(symbol.into())
            .or_default()
            .apply(data, kind, ts, market)
            .map(DataEvent::OrderBook)
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
    run_bybit_topic_loop(bybit_url(market), topics, ctx, move |data, _, _| {
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
        |data, _, _| {
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
    F: FnMut(&serde_json::Value, Option<&str>, Option<u64>) -> Option<DataEvent>,
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

    let mut last_pong = Instant::now();
    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                if last_pong.elapsed() > Duration::from_secs(60) { bail!("bybit topic pong timeout"); }
                sink.send(Message::Text(json!({"op":"ping"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange: "bybit", ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("bybit topic stream ended")??;
                match msg {
                    Message::Text(text) => {
                        if let Ok(m) = serde_json::from_str::<BybitMsg>(&text) {
                            if m.op.as_deref() == Some("pong") || m.ret_msg.as_deref() == Some("pong") {
                                last_pong = Instant::now(); continue;
                            }
                            let ts=m.ts.as_ref().and_then(|v|v.as_u64().or_else(||v.as_str().and_then(|s|s.parse().ok())));
                            let Some(data)=m.data else {continue;};
                            match data {
                                serde_json::Value::Array(items) => {
                                    for item in items {
                                        if let Some(event) = build_event(&item,m.message_type.as_deref(),ts) {
                                            ctx.emit(event).await?;
                                        }
                                    }
                                }
                                item => {
                                    if let Some(event) = build_event(&item,m.message_type.as_deref(),ts) {
                                        ctx.emit(event).await?;
                                    }
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("bybit topic stream closed"),
                    Message::Pong(_) => last_pong=Instant::now(),
                    Message::Binary(_) | Message::Frame(_) => {}
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
    use super::*;
    use crate::types::TradeSide;

    #[test]
    fn bybit_side_parser_accepts_api_labels() {
        assert_eq!(side_from_str("Buy"), TradeSide::Buy);
        assert_eq!(side_from_str("Sell"), TradeSide::Sell);
        assert_eq!(side_from_str("other"), TradeSide::Unknown);
    }

    fn snapshot() -> serde_json::Value {
        json!({"s":"BTCUSDT","u":10,"b":[["99","2"],["98","3"]],"a":[["101","2"],["102","3"]]})
    }

    #[test]
    fn depth_deltas_preserve_unchanged_levels_and_remove_zero_size() {
        let mut book = BybitBook::default();
        book.apply(&snapshot(), Some("snapshot"), Some(1000), MarketKind::Spot)
            .unwrap();
        let delta = json!({"s":"BTCUSDT","u":12,"b":[["99","0"],["98.5","1"]],"a":[["101","4"]]});
        let tick = book
            .apply(&delta, Some("delta"), Some(1020), MarketKind::Spot)
            .unwrap();
        assert_eq!(
            tick.bids
                .iter()
                .map(|l| (l.price, l.qty))
                .collect::<Vec<_>>(),
            vec![(98.5, 1.0), (98.0, 3.0)]
        );
        assert_eq!(
            tick.asks
                .iter()
                .map(|l| (l.price, l.qty))
                .collect::<Vec<_>>(),
            vec![(101.0, 4.0), (102.0, 3.0)]
        );
        assert_eq!(tick.ts_ms, 1020);
        assert_eq!(tick.last_update_id, Some(12));
        // Update IDs need not be consecutive; duplicates/older deltas do not mutate state.
        assert!(
            book.apply(&delta, Some("delta"), Some(1030), MarketKind::Spot)
                .is_none()
        );
        assert_eq!(book.bids[0].price, 98.5);
    }

    #[test]
    fn depth_requires_snapshot_and_restart_resets_all_levels() {
        let mut book = BybitBook::default();
        assert!(
            book.apply(&snapshot(), Some("delta"), None, MarketKind::Spot)
                .is_none()
        );
        book.apply(&snapshot(), Some("snapshot"), None, MarketKind::Spot)
            .unwrap();
        let reset = json!({"s":"BTCUSDT","u":1,"b":[["95","1"]],"a":[["105","1"]]});
        let tick = book
            .apply(&reset, Some("delta"), Some(2000), MarketKind::Spot)
            .unwrap();
        assert_eq!(tick.bids.len(), 1);
        assert_eq!(tick.asks.len(), 1);
        assert_eq!(tick.bids[0].price, 95.0);
    }

    #[test]
    fn invalid_depth_invalidates_builder_until_next_snapshot() {
        let mut book = BybitBook::default();
        book.apply(&snapshot(), Some("snapshot"), None, MarketKind::Spot)
            .unwrap();
        let crossed = json!({"s":"BTCUSDT","u":11,"b":[["110","1"]],"a":[]});
        assert!(
            book.apply(&crossed, Some("delta"), None, MarketKind::Spot)
                .is_none()
        );
        assert!(book.update_id.is_none());
        assert!(
            book.apply(&snapshot(), Some("delta"), None, MarketKind::Spot)
                .is_none()
        );
        assert!(
            book.apply(&snapshot(), Some("snapshot"), None, MarketKind::Spot)
                .is_some()
        );
    }
}
