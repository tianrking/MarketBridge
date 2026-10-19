//! Live protocol drivers. Each instrument reconnects independently; transport
//! termination drops its lease and immediately invalidates cached old sessions.
use crate::{
    book_integrity::{Applied, BookLease, LocalBook, Protocol, parse_update},
    source::SourceContext,
    types::{DataEvent, MarketKind},
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt, future::try_join_all};
use std::{collections::VecDeque, time::Duration};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub async fn run(
    protocol: Protocol,
    market: MarketKind,
    symbols: &[String],
    ctx: SourceContext,
) -> Result<()> {
    ensure!(!symbols.is_empty(), "depth instruments empty");
    ensure!(
        symbols.len() <= ctx.metrics.books.config().max_symbols,
        "depth symbols exceed configured global budget"
    );
    try_join_all(symbols.iter().map(|symbol| {
        let ctx=ctx.clone();async move {
            let mut delay=500;
            loop {
                let lease=ctx.metrics.books.begin(protocol.exchange(),market,symbol)?;
                let started=std::time::Instant::now();
                let result=run_one(protocol,market,symbol,&ctx,&lease,None).await;
                lease.invalidate("protocol_or_transport_failure");
                audit(&ctx,protocol,market,symbol,&lease,"resynchronization_required",None).await?;
                if let Err(error)=result {tracing::warn!(exchange=protocol.exchange(),symbol,error=%error,"order book invalidated; instrument will resynchronize");}
                if started.elapsed()>Duration::from_secs(60){delay=500;}
                tokio::time::sleep(Duration::from_millis(delay)).await;delay=(delay*2).min(30000);
            }
            #[allow(unreachable_code)] Ok::<(),anyhow::Error>(())
        }
    })).await?;
    Ok(())
}

fn endpoint(protocol: Protocol, market: MarketKind, symbol: &str) -> (String, Option<String>) {
    match protocol {
        Protocol::BinanceSpot => (
            format!(
                "wss://stream.binance.com:9443/ws/{}@depth@100ms",
                symbol.to_ascii_lowercase()
            ),
            Some(format!(
                "https://api.binance.com/api/v3/depth?symbol={symbol}&limit=1000"
            )),
        ),
        Protocol::BinancePerp => (
            format!(
                "wss://fstream.binance.com/public/ws/{}@depth@100ms",
                symbol.to_ascii_lowercase()
            ),
            Some(format!(
                "https://fapi.binance.com/fapi/v1/depth?symbol={symbol}&limit=1000"
            )),
        ),
        Protocol::Okx => ("wss://ws.okx.com:8443/ws/v5/public".into(), None),
        Protocol::Bybit => (
            format!(
                "wss://stream.bybit.com/v5/public/{}",
                if market == MarketKind::Spot {
                    "spot"
                } else {
                    "linear"
                }
            ),
            None,
        ),
    }
}
async fn snapshot(ctx: &SourceContext, url: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(
            ctx.metrics.books.config().snapshot_timeout_ms,
        ))
        .build()?;
    let mut response = client.get(url).send().await?;
    if matches!(response.status().as_u16(), 429 | 418) {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .clamp(1, 3600);
        ctx.metrics.books.throttle(Duration::from_secs(retry));
        bail!("depth snapshot rate limited; shared request budget deferred");
    }
    response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 4 * 1024 * 1024,
            "depth snapshot body exceeds byte budget"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn contract_value(symbol: &str, value: &serde_json::Value) -> Result<f64> {
    ensure!(value["code"] == "0", "OKX instrument request rejected");
    let rows = value["data"]
        .as_array()
        .context("missing OKX instrument metadata")?;
    let instrument = rows
        .iter()
        .find(|v| v["instId"] == symbol)
        .context("OKX instrument metadata missing requested id")?;
    let base = symbol.split('-').next().context("invalid instrument id")?;
    ensure!(
        instrument["ctType"] == "linear"
            && instrument["ctValCcy"] == base
            && instrument["settleCcy"] == "USDT",
        "only verified base-denominated linear USDT contract units can be normalized"
    );
    let scalar = |key: &str| -> Result<f64> {
        let n = instrument[key]
            .as_str()
            .context("missing contract scalar")?
            .parse::<f64>()?;
        ensure!(n.is_finite() && n > 0.0, "invalid contract scalar");
        Ok(n)
    };
    ensure!(
        scalar("ctMult")? == 1.0,
        "non-unit contract multiplier requires explicit instrument support"
    );
    scalar("ctVal")
}

async fn run_one(
    protocol: Protocol,
    market: MarketKind,
    symbol: &str,
    ctx: &SourceContext,
    lease: &BookLease,
    override_endpoints: Option<(String, Option<String>)>,
) -> Result<()> {
    let config = ctx.metrics.books.config();
    let (ws_url, rest) = override_endpoints.unwrap_or_else(|| endpoint(protocol, market, symbol));
    if rest.is_some() {
        ctx.metrics
            .books
            .request_weight(if protocol == Protocol::BinanceSpot {
                50
            } else {
                20
            })
            .await;
    }
    let unit = if protocol == Protocol::Okx && market == MarketKind::Perp {
        ctx.metrics.books.request_weight(2).await;
        let url =
            format!("https://www.okx.com/api/v5/public/instruments?instType=SWAP&instId={symbol}");
        Some(contract_value(symbol, &snapshot(ctx, &url).await?)?)
    } else {
        None
    };
    let ws_config = WebSocketConfig {
        max_message_size: Some(4 * 1024 * 1024),
        max_frame_size: Some(4 * 1024 * 1024),
        ..Default::default()
    };
    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(15),
        connect_async_with_config(&ws_url, Some(ws_config), false),
    )
    .await
    .context("depth websocket connection timed out")??;
    let (mut sink, mut stream) = ws.split();
    match protocol {
        Protocol::Okx => {
            sink.send(Message::Text(
                serde_json::json!({"op":"subscribe","args":[{"channel":"books","instId":symbol}]})
                    .to_string(),
            ))
            .await?
        }
        Protocol::Bybit => {
            sink.send(Message::Text(
                serde_json::json!({"op":"subscribe","args":[format!("orderbook.50.{symbol}")]})
                    .to_string(),
            ))
            .await?
        }
        _ => {}
    }
    let mut book = LocalBook::new(
        protocol,
        market,
        symbol.into(),
        lease.session.clone(),
        config.clone(),
    );
    if let Some(value) = unit {
        book.set_contract_value(value)?;
    }
    let needs_rest = rest.is_some();
    let mut snapshot_done = !needs_rest;
    let mut fetch = Box::pin(async {
        if let Some(url) = rest {
            snapshot(ctx, &url).await
        } else {
            futures_util::future::pending().await
        }
    });
    let mut buffered = VecDeque::<(serde_json::Value, usize)>::new();
    let mut buffered_bytes = 0;
    let mut last_receive = tokio::time::Instant::now();
    let mut ping = tokio::time::interval(Duration::from_secs(20));
    loop {
        tokio::select! {
            result=&mut fetch,if !snapshot_done=>{
                let value=result?;book.apply(&parse_update(protocol,&value,true)?)?;snapshot_done=true;
                while let Some((value,_))=buffered.pop_front() {apply_publish(protocol,&value,false,&mut book,ctx,lease).await?;}
                buffered_bytes=0;lease.buffered(0)?;
            }
            _=ping.tick()=>{
                ensure!(last_receive.elapsed()<Duration::from_secs(60),"depth transport idle timeout");
                let msg=match protocol {Protocol::Okx=>Message::Text("ping".into()),Protocol::Bybit=>Message::Text(serde_json::json!({"op":"ping"}).to_string()),_=>Message::Ping(Vec::new())};
                sink.send(msg).await?;
            }
            message=stream.next()=>{
                let message=message.context("depth websocket ended")??;last_receive=tokio::time::Instant::now();
                match message {
                    Message::Ping(payload)=>{sink.send(Message::Pong(payload)).await?;continue;},
                    Message::Pong(_)=>{ctx.emit(DataEvent::Heartbeat{exchange:protocol.exchange(),ts_ms:crate::types::now_ms()}).await?;continue;},Message::Close(_)=>bail!("depth websocket closed"),Message::Frame(_)=>continue,
                    Message::Text(_) | Message::Binary(_)=>{},
                }
                let text=super::ws::message_text(&message)?.context("missing depth text")?;ensure!(text.len()<=4*1024*1024,"depth message exceeds byte budget");
                if text=="pong" {continue;}
                let envelope:serde_json::Value=serde_json::from_str(&text)?;
                if protocol==Protocol::Okx {
                    if envelope["event"]=="error" {bail!("OKX depth subscription rejected");}
                    if envelope.get("event").is_some(){continue;}
                    ensure!(envelope["arg"]["channel"]=="books" && envelope["arg"]["instId"]==symbol,"unexpected OKX book channel/instrument");
                    let snapshot=match envelope["action"].as_str(){Some("snapshot")=>true,Some("update")=>false,_=>bail!("unknown OKX book action")};
                    let rows=envelope["data"].as_array().context("missing OKX book data")?;ensure!(!rows.is_empty()&&rows.len()<=16,"invalid OKX batch");
                    for value in rows {apply_publish(protocol,value,snapshot,&mut book,ctx,lease).await?;}
                }else if protocol==Protocol::Bybit {
                    if envelope.get("op").is_some(){ensure!(envelope["success"]!=false,"Bybit depth subscription rejected");continue;}
                    ensure!(envelope["topic"]==format!("orderbook.50.{symbol}"),"unexpected Bybit book topic");
                    let mut value=envelope["data"].clone();ensure!(value["s"]==symbol,"unexpected Bybit book instrument");
                    value["ts"]=envelope["ts"].clone();
                    let snapshot=match envelope["type"].as_str(){Some("snapshot")=>true,Some("delta")=>false,_=>bail!("unknown Bybit book action")};
                    apply_publish(protocol,&value,snapshot,&mut book,ctx,lease).await?;
                }else {
                    ensure!(envelope["e"]=="depthUpdate" && envelope["s"]==symbol,"unexpected Binance depth instrument");
                    if snapshot_done {apply_publish(protocol,&envelope,false,&mut book,ctx,lease).await?;}
                    else {
                        buffered_bytes+=text.len();ensure!(buffered.len()<config.max_buffered_updates && buffered_bytes<=config.max_buffered_bytes,"depth bootstrap buffer exhausted");
                        lease.buffered(buffered_bytes)?;buffered.push_back((envelope,text.len()));
                    }
                }
            }
        }
    }
}
async fn apply_publish(
    protocol: Protocol,
    value: &serde_json::Value,
    snapshot: bool,
    book: &mut LocalBook,
    ctx: &SourceContext,
    lease: &BookLease,
) -> Result<()> {
    let update = parse_update(protocol, value, snapshot)?;
    if book.apply(&update)? == Applied::Publish {
        let recovered = update.snapshot || !lease.is_synchronized();
        let tick = book.tick(update.checksum)?;
        let proof=tick.book_quality.as_ref().map(|q|serde_json::json!({"snapshot_sequence":q.snapshot_sequence,"native_sequence":q.native_sequence,"snapshot_generation":q.snapshot_generation,"evidence_crc32":q.evidence_crc32,"checksum_policy":q.checksum_policy}));
        lease.synchronized(&tick, book.owned_bytes())?;
        // Full reconstructed views make downstream broadcast skips visible, but
        // do not require downstream consumers to merge a missing delta.
        let symbol = tick.symbol.clone();
        let market = tick.market;
        ctx.emit(DataEvent::OrderBook(tick)).await?;
        if recovered {
            audit(ctx, protocol, market, &symbol, lease, "synchronized", proof).await?;
        }
    }
    Ok(())
}

async fn audit(
    ctx: &SourceContext,
    protocol: Protocol,
    market: MarketKind,
    symbol: &str,
    lease: &BookLease,
    status: &str,
    proof: Option<serde_json::Value>,
) -> Result<()> {
    ctx.emit(DataEvent::ExternalSignal(crate::types::ExternalSignalTick{source:protocol.exchange(),source_instance:Some(lease.session.clone().into()),source_time_ms:None,category:"order_book_integrity".into(),symbol:Some(symbol.into()),metric:status.into(),value:Some(if status=="synchronized"{1.0}else{0.0}),score:None,title:None,url:None,ts_ms:crate::types::now_ms(),raw:Some(serde_json::json!({"session":lease.session,"market":market,"status":status,"proof":proof,"scope":"current bounded reconstructed book; historical OFI must warm up separately"}))})).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{metrics::AppMetrics, types::BackpressureMode};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    async fn wire_case(
        protocol: Protocol,
        frames: Vec<serde_json::Value>,
        expected: usize,
    ) -> (
        Vec<crate::types::OrderBookTick>,
        crate::book_integrity::BookRegistry,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            for frame in frames {
                ws.send(Message::Text(frame.to_string())).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let rest = if matches!(protocol, Protocol::BinanceSpot | Protocol::BinancePerp) {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut req = [0; 4096];
                let _ = socket.read(&mut req).await.unwrap();
                tokio::time::sleep(Duration::from_millis(30)).await;
                let body = r#"{"lastUpdateId":10,"bids":[["99","1"]],"asks":[["101","2"]]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            Some(format!("http://{addr}/depth"))
        } else {
            None
        };
        let metrics = AppMetrics::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        metrics.ingestion.register("fixture#0", "fixture").unwrap();
        metrics.ingestion.starting("fixture#0");
        let ctx = SourceContext {
            tx,
            source_id: "fixture#0".into(),
            backpressure: BackpressureMode::Block,
            metrics: metrics.clone(),
        };
        let lease = metrics
            .books
            .begin(protocol.exchange(), MarketKind::Spot, "BTCUSDT")
            .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            run_one(
                protocol,
                MarketKind::Spot,
                "BTCUSDT",
                &ctx,
                &lease,
                Some((format!("ws://{addr}"), rest)),
            ),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        lease.invalidate("injected_failure");
        drop(lease);
        drop(ctx);
        let mut books = vec![];
        while let Some(m) = rx.recv().await {
            if let DataEvent::OrderBook(b) = m.event {
                books.push(b);
            }
        }
        assert_eq!(books.len(), expected);
        assert!(books.iter().all(|b| !metrics.books.usable(b)));
        peer.await.unwrap();
        (books, metrics.books.clone())
    }
    #[test]
    fn instrument_units_are_explicit_and_inverse_or_missing_contracts_rejected() {
        let mut value = serde_json::json!({"code":"0","data":[{"instId":"BTC-USDT-SWAP","ctType":"linear","ctValCcy":"BTC","settleCcy":"USDT","ctVal":"0.01","ctMult":"1"}]});
        assert_eq!(contract_value("BTC-USDT-SWAP", &value).unwrap(), 0.01);
        value["data"][0]["ctType"] = "inverse".into();
        assert!(contract_value("BTC-USDT-SWAP", &value).is_err());
        value["data"][0]["ctType"] = "linear".into();
        value["data"][0]["ctValCcy"] = "USD".into();
        assert!(contract_value("BTC-USDT-SWAP", &value).is_err());
        value["data"][0]["ctValCcy"] = "BTC".into();
        value["data"][0]["ctMult"] = "".into();
        assert!(contract_value("BTC-USDT-SWAP", &value).is_err());
    }
    #[tokio::test]
    async fn real_socket_binance_buffers_snapshot_and_rejects_gap() {
        let frames = vec![
            serde_json::json!({"e":"depthUpdate","s":"BTCUSDT","E":crate::types::now_ms(),"U":11,"u":11,"b":[["99","3"]],"a":[]}),
            serde_json::json!({"e":"depthUpdate","s":"BTCUSDT","E":crate::types::now_ms(),"U":13,"u":13,"b":[],"a":[]}),
        ];
        let (books, _) = wire_case(Protocol::BinanceSpot, frames, 1).await;
        assert_eq!(books[0].bids[0].qty, 3.0);
        assert_eq!(
            books[0].book_quality.as_ref().unwrap().snapshot_sequence,
            10
        );
    }
    #[tokio::test]
    async fn real_socket_okx_checksum_corruption_invalidates_cached_session() {
        let mut crc = flate2::Crc::new();
        crc.update(b"99:1:101:2");
        let checksum = crc.sum() as i32;
        let frames = vec![
            serde_json::json!({"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seqId":10,"prevSeqId":-1,"ts":crate::types::now_ms().to_string(),"bids":[["99","1","0","1"]],"asks":[["101","2","0","1"]],"checksum":checksum}]}),
            serde_json::json!({"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"seqId":11,"prevSeqId":10,"ts":crate::types::now_ms().to_string(),"bids":[["99","3","0","1"]],"asks":[],"checksum":checksum}]}),
        ];
        wire_case(Protocol::Okx, frames, 1).await;
    }
    #[tokio::test]
    async fn real_socket_bybit_monotonic_ids_and_restart_require_snapshot() {
        let frames = vec![
            serde_json::json!({"topic":"orderbook.50.BTCUSDT","type":"snapshot","ts":crate::types::now_ms(),"data":{"s":"BTCUSDT","u":10,"seq":20,"b":[["99","1"]],"a":[["101","2"]]}}),
            serde_json::json!({"topic":"orderbook.50.BTCUSDT","type":"delta","ts":crate::types::now_ms(),"data":{"s":"BTCUSDT","u":15,"seq":30,"b":[["99","0"],["100","3"]],"a":[]}}),
            serde_json::json!({"topic":"orderbook.50.BTCUSDT","type":"delta","ts":crate::types::now_ms(),"data":{"s":"BTCUSDT","u":1,"seq":1,"b":[],"a":[]}}),
        ];
        let (books, _) = wire_case(Protocol::Bybit, frames, 2).await;
        assert_eq!(books[1].bids[0].price, 100.0);
    }
}
