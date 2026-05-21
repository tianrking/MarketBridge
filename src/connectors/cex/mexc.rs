use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::time::{Instant, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

use crate::connectors::cex::common::{
    emit_tick_ext, first_str, parse_array_levels, side_from_labels,
};
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{
    DataEvent, FundingRateTick, MarketKind, OrderBookTick, TradeSide, TradeTick, now_ms,
};

const MEXC_CONTRACT_REST_URL: &str = "https://api.mexc.com/api/v1/contract";

pub struct MexcFeed {
    market: MarketKind,
    symbols: Vec<String>,
}

impl MexcFeed {
    pub fn new(market: MarketKind, symbols: Vec<String>) -> Self {
        Self { market, symbols }
    }
}

pub struct MexcFundingPoller {
    symbols: Vec<String>,
    client: reqwest::Client,
}

impl MexcFundingPoller {
    pub fn new(symbols: Vec<String>) -> Self {
        Self {
            symbols,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ExchangeSource for MexcFundingPoller {
    fn name(&self) -> &'static str {
        "mexc"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        if self.symbols.is_empty() {
            bail!("mexc funding symbols empty");
        }

        let mut tick = interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            for symbol in &self.symbols {
                match poll_mexc_funding(&self.client, symbol).await {
                    Ok(Some(event)) => ctx.emit(event).await?,
                    Ok(None) => {}
                    Err(err) => {
                        warn!(exchange = "mexc", symbol, error = %err, "funding poll failed")
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ExchangeSource for MexcFeed {
    fn name(&self) -> &'static str {
        "mexc"
    }

    async fn run(&self, ctx: SourceContext) -> Result<()> {
        match self.market {
            MarketKind::Spot => run_mexc_spot(&self.symbols, ctx).await,
            MarketKind::Perp => run_mexc_contract(&self.symbols, ctx).await,
        }
    }
}

async fn poll_mexc_funding(client: &reqwest::Client, symbol: &str) -> Result<Option<DataEvent>> {
    let response = client
        .get(format!("{MEXC_CONTRACT_REST_URL}/funding_rate/{symbol}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    Ok(parse_mexc_funding(&response))
}

async fn run_mexc_spot(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("mexc spot symbols empty");
    }
    let (ws, _) = connect_async("wss://wbs.mexc.com/ws")
        .await
        .context("mexc spot connect failed")?;
    let (mut sink, stream) = ws.split();
    let params = symbols
        .iter()
        .flat_map(|symbol| {
            [
                format!("spot@public.bookTicker.v3.api@{symbol}"),
                format!("spot@public.limit.depth.v3.api@{symbol}@20"),
                format!("spot@public.deals.v3.api@{symbol}"),
            ]
        })
        .collect::<Vec<_>>();
    sink.send(Message::Text(
        json!({"method":"SUBSCRIPTION","params":params}).to_string(),
    ))
    .await?;
    run_json_loop("mexc", MarketKind::Spot, sink, stream, ctx).await
}

async fn run_mexc_contract(symbols: &[String], ctx: SourceContext) -> Result<()> {
    if symbols.is_empty() {
        bail!("mexc perp symbols empty");
    }
    let (ws, _) = connect_async("wss://contract.mexc.com/edge")
        .await
        .context("mexc contract connect failed")?;
    let (mut sink, stream) = ws.split();
    for symbol in symbols {
        for method in ["sub.ticker", "sub.depth.full", "sub.deal"] {
            sink.send(Message::Text(
                json!({"method":method,"param":{"symbol":symbol}}).to_string(),
            ))
            .await?;
        }
    }
    run_json_loop("mexc", MarketKind::Perp, sink, stream, ctx).await
}

async fn run_json_loop<S>(
    exchange: &'static str,
    market: MarketKind,
    mut sink: S,
    mut stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    ctx: SourceContext,
) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    <S as futures_util::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
{
    let mut ping = interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if last_seen.elapsed() > Duration::from_secs(90) {
                    bail!("mexc pong timeout");
                }
                sink.send(Message::Text(json!({"method":"PING"}).to_string())).await?;
                ctx.emit(DataEvent::Heartbeat { exchange, ts_ms: now_ms() }).await?;
            }
            msg = stream.next() => {
                let msg = msg.context("mexc stream ended")??;
                match msg {
                    Message::Text(text) => {
                        last_seen = Instant::now();
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            for event in parse_mexc_events(market, &value, &ctx).await? {
                                ctx.emit(event).await?;
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        last_seen = Instant::now();
                        sink.send(Message::Pong(payload)).await?;
                    }
                    Message::Close(_) => bail!("mexc closed"),
                    Message::Pong(_) | Message::Binary(_) => {
                        last_seen = Instant::now();
                    }
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

async fn parse_mexc_events(
    market: MarketKind,
    value: &Value,
    ctx: &SourceContext,
) -> Result<Vec<DataEvent>> {
    let channel = value
        .get("c")
        .or_else(|| value.get("channel"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = value
        .get("d")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    let symbol = value
        .get("s")
        .or_else(|| value.get("symbol"))
        .or_else(|| data.get("symbol"))
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    if channel.contains("bookTicker") || channel.contains("ticker") {
        let bid = first_str(data, &["bidPrice", "bid", "b", "bid1"]).unwrap_or("0");
        let ask = first_str(data, &["askPrice", "ask", "a", "ask1"]).unwrap_or("0");
        emit_tick_ext(
            ctx,
            "mexc",
            market,
            symbol,
            bid,
            ask,
            None,
            first_str(data, &["fundingRate"]),
            None,
        )
        .await?;
        return Ok(Vec::new());
    }
    if channel.contains("depth") {
        return Ok(vec![DataEvent::OrderBook(OrderBookTick {
            exchange: "mexc",
            market,
            symbol: symbol.to_string().into_boxed_str(),
            bids: data
                .get("bids")
                .or_else(|| data.get("b"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            asks: data
                .get("asks")
                .or_else(|| data.get("a"))
                .and_then(Value::as_array)
                .map(|x| parse_array_levels(x))
                .unwrap_or_default(),
            last_update_id: None,
            ts_ms: value
                .get("t")
                .and_then(Value::as_u64)
                .unwrap_or_else(now_ms),
        })]);
    }
    if channel.contains("deal") || channel.contains("trade") {
        let items = data
            .get("deals")
            .and_then(Value::as_array)
            .or_else(|| data.as_array());
        return Ok(items
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let symbol = first_value_str(item, &["s", "symbol", "symbolName"])
                    .unwrap_or(symbol)
                    .to_string()
                    .into_boxed_str();
                let trade_id = first_value_string(item, &["id", "tradeId"])
                    .or_else(|| first_value_string(item, &["t"]))
                    .map(String::into_boxed_str);
                let ts_ms = first_value_u64(item, &["T", "time", "ts", "createTime"])
                    .or_else(|| first_value_u64(item, &["t"]).filter(|ts| *ts > 1_000_000_000_000))
                    .unwrap_or_else(now_ms);
                Some(DataEvent::Trade(TradeTick {
                    exchange: "mexc",
                    market,
                    symbol,
                    price: first_value_f64(item, &["p", "price"])?,
                    qty: first_value_f64(item, &["v", "q", "quantity", "vol"])?,
                    side: side_from_str(
                        first_value_string(item, &["S", "side", "tradeType", "T"])
                            .as_deref()
                            .unwrap_or_default(),
                    ),
                    trade_id,
                    ts_ms,
                }))
            })
            .collect());
    }
    Ok(Vec::new())
}

fn side_from_str(side: &str) -> TradeSide {
    side_from_labels(side, &["1", "buy"], &["2", "sell"])
}

fn parse_mexc_funding(value: &Value) -> Option<DataEvent> {
    let data = value.get("data").unwrap_or(value);
    let funding_rate = first_value_f64(data, &["fundingRate", "rate"])?;
    Some(DataEvent::FundingRate(FundingRateTick {
        exchange: "mexc",
        symbol: first_value_str(data, &["symbol"])
            .unwrap_or("UNKNOWN")
            .to_string()
            .into_boxed_str(),
        funding_rate,
        next_funding_time_ms: first_value_u64(data, &["nextSettleTime"]),
        mark_price: None,
        index_price: None,
        ts_ms: first_value_u64(data, &["timestamp"]).unwrap_or_else(now_ms),
    }))
}

fn first_value_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value.get(*key)?.as_str())
}

fn first_value_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = value.get(*key)?;
        value
            .as_str()
            .map(str::to_string)
            .or_else(|| value.as_i64().map(|n| n.to_string()))
            .or_else(|| value.as_u64().map(|n| n.to_string()))
    })
}

fn first_value_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        let value = value.get(*key)?;
        value
            .as_f64()
            .or_else(|| value.as_str()?.parse::<f64>().ok())
    })
}

fn first_value_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let value = value.get(*key)?;
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| value.as_str()?.parse::<u64>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        first_value_f64, first_value_string, first_value_u64, parse_mexc_funding, side_from_str,
    };
    use crate::types::{DataEvent, TradeSide};
    use serde_json::json;

    #[test]
    fn mexc_side_parser_accepts_common_labels() {
        assert_eq!(side_from_str("1"), TradeSide::Buy);
        assert_eq!(side_from_str("2"), TradeSide::Sell);
        assert_eq!(side_from_str("?"), TradeSide::Unknown);
    }

    #[test]
    fn mexc_trade_helpers_accept_numeric_wire_values() {
        let item = json!({"p": 101.5, "v": "2.25", "T": 2, "id": 42, "time": "1710000000000"});

        assert_eq!(first_value_f64(&item, &["p"]), Some(101.5));
        assert_eq!(first_value_f64(&item, &["v"]), Some(2.25));
        assert_eq!(first_value_string(&item, &["T"]).as_deref(), Some("2"));
        assert_eq!(first_value_string(&item, &["id"]).as_deref(), Some("42"));
        assert_eq!(first_value_u64(&item, &["time"]), Some(1_710_000_000_000));
    }

    #[test]
    fn mexc_funding_parser_accepts_contract_payload() {
        let event = parse_mexc_funding(&json!({
            "success": true,
            "code": 0,
            "data": {
                "symbol": "BTC_USDT",
                "fundingRate": 0.000014,
                "nextSettleTime": 1643241600000_u64,
                "timestamp": 1643240373359_u64
            }
        }))
        .expect("funding event");

        match event {
            DataEvent::FundingRate(tick) => {
                assert_eq!(tick.symbol.as_ref(), "BTC_USDT");
                assert_eq!(tick.funding_rate, 0.000014);
                assert_eq!(tick.next_funding_time_ms, Some(1643241600000));
                assert_eq!(tick.ts_ms, 1643240373359);
            }
            _ => panic!("unexpected event type"),
        }
    }
}
