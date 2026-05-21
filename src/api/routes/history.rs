use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::Value;

use crate::api::ApiState;
use crate::klines::{KlineBar, interval_to_ms};
use crate::types::now_ms;

#[derive(Debug, Deserialize, Default)]
pub struct HistoryCandlesQuery {
    exchange: String,
    symbol: String,
    interval: Option<String>,
    market: Option<String>,
    candle_type: Option<String>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    limit: Option<usize>,
    persist: Option<bool>,
}

pub async fn candles(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<HistoryCandlesQuery>,
) -> impl IntoResponse {
    let candle_type = q
        .candle_type
        .as_deref()
        .unwrap_or(q.market.as_deref().unwrap_or("spot"))
        .trim()
        .to_ascii_lowercase();
    let result = match q.exchange.trim().to_ascii_lowercase().as_str() {
        "binance" => fetch_binance_history(&state.http, &q, &candle_type).await,
        "okx" => fetch_okx_history(&state.http, &q, &candle_type).await,
        other => Err(anyhow::anyhow!("unsupported history exchange: {other}")),
    };

    match result {
        Ok(mut rows) => {
            rows.sort_by_key(|row| row.open_time_ms);
            let persist_result = if q.persist.unwrap_or(false) {
                match state
                    .data_lake_store
                    .persist_klines(rows.clone(), candle_type.clone())
                    .await
                {
                    Ok(partitions) => serde_json::json!({"ok": true, "partitions": partitions}),
                    Err(error) => serde_json::json!({"ok": false, "error": error.to_string()}),
                }
            } else {
                serde_json::json!({"ok": false, "reason": "persist_query_param_not_set"})
            };
            Json(serde_json::json!({
                "version": "v1",
                "domain": "history_candles",
                "exchange": q.exchange,
                "symbol": q.symbol,
                "candle_type": candle_type,
                "persist": persist_result,
                "candles": rows
            }))
        }
        Err(error) => Json(serde_json::json!({
            "version": "v1",
            "domain": "history_candles",
            "error": error.to_string(),
            "candles": []
        })),
    }
}

async fn fetch_binance_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type == "funding_rate" {
        return fetch_binance_funding_rate(http, q).await;
    }
    let interval = q.interval.as_deref().unwrap_or("1m");
    let limit = q.limit.unwrap_or(500).clamp(1, 1500).to_string();
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let (url, symbol_key, market, source) = match candle_type {
        "spot" => (
            "https://api.binance.com/api/v3/klines",
            "symbol",
            "spot",
            "binance_spot_klines",
        ),
        "futures" | "perp" => (
            "https://fapi.binance.com/fapi/v1/klines",
            "symbol",
            "perp",
            "binance_futures_klines",
        ),
        "mark" => (
            "https://fapi.binance.com/fapi/v1/markPriceKlines",
            "symbol",
            "perp",
            "binance_mark_price_klines",
        ),
        "index" => (
            "https://fapi.binance.com/fapi/v1/indexPriceKlines",
            "pair",
            "perp",
            "binance_index_price_klines",
        ),
        "premiumindex" | "premium_index" | "premium" => (
            "https://fapi.binance.com/fapi/v1/premiumIndexKlines",
            "symbol",
            "perp",
            "binance_premium_index_klines",
        ),
        other => bail!("unsupported binance candle_type: {other}"),
    };
    let mut request = http.get(url).query(&[
        (symbol_key, symbol.as_str()),
        ("interval", interval),
        ("limit", limit.as_str()),
    ]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Vec<Value>>>()
        .await
        .context("failed to parse binance historical candles")?;
    payload
        .into_iter()
        .map(|row| {
            parse_binance_array_row(
                "binance",
                market,
                &symbol,
                interval,
                source,
                row,
                matches!(candle_type, "spot" | "futures" | "perp"),
            )
        })
        .collect()
}

async fn fetch_binance_funding_rate(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
) -> Result<Vec<KlineBar>> {
    let symbol = q.symbol.trim().to_ascii_uppercase();
    let limit = q.limit.unwrap_or(500).clamp(1, 1000).to_string();
    let mut request = http
        .get("https://fapi.binance.com/fapi/v1/fundingRate")
        .query(&[("symbol", symbol.as_str()), ("limit", limit.as_str())]);
    if let Some(start_ms) = q.start_ms {
        request = request.query(&[("startTime", start_ms.to_string())]);
    }
    if let Some(end_ms) = q.end_ms {
        request = request.query(&[("endTime", end_ms.to_string())]);
    }
    let payload = request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Value>>()
        .await
        .context("failed to parse binance funding history")?;
    let interval = q.interval.as_deref().unwrap_or("8h");
    let interval_ms = interval_to_ms(interval).unwrap_or(28_800_000);
    payload
        .into_iter()
        .map(|row| {
            let open_time_ms = value_u64(row.get("fundingTime")).context("missing fundingTime")?;
            let funding_rate = value_f64(row.get("fundingRate")).context("missing fundingRate")?;
            Ok(KlineBar {
                exchange: "binance".to_string(),
                market: "perp".to_string(),
                symbol: symbol.clone(),
                interval: interval.to_string(),
                open_time_ms,
                close_time_ms: open_time_ms + interval_ms - 1,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "binance_funding_rate_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect()
}

async fn fetch_okx_history(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
    candle_type: &str,
) -> Result<Vec<KlineBar>> {
    if candle_type == "funding_rate" {
        return fetch_okx_funding_rate(http, q).await;
    }
    let interval = q.interval.as_deref().unwrap_or("1m");
    let limit = q.limit.unwrap_or(300).clamp(1, 300).to_string();
    let market = q.market.as_deref().unwrap_or("perp");
    let inst_id = okx_inst_id(&q.symbol, market);
    let (url, source, has_volume) = match candle_type {
        "spot" | "futures" | "perp" => (
            "https://www.okx.com/api/v5/market/candles",
            "okx_market_candles",
            true,
        ),
        "mark" => (
            "https://www.okx.com/api/v5/market/mark-price-candles",
            "okx_mark_price_candles",
            false,
        ),
        "index" => (
            "https://www.okx.com/api/v5/market/index-candles",
            "okx_index_candles",
            false,
        ),
        other => bail!("unsupported okx candle_type: {other}"),
    };
    let payload = http
        .get(url)
        .query(&[
            ("instId", inst_id.as_str()),
            ("bar", interval),
            ("limit", limit.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx candles")?;
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            parse_okx_array_row(
                "okx",
                if market == "spot" { "spot" } else { "perp" },
                &q.symbol,
                interval,
                source,
                row,
                has_volume,
            )
        })
        .collect()
}

async fn fetch_okx_funding_rate(
    http: &reqwest::Client,
    q: &HistoryCandlesQuery,
) -> Result<Vec<KlineBar>> {
    let inst_id = okx_inst_id(&q.symbol, "perp");
    let limit = q.limit.unwrap_or(100).clamp(1, 100).to_string();
    let payload = http
        .get("https://www.okx.com/api/v5/public/funding-rate-history")
        .query(&[("instId", inst_id.as_str()), ("limit", limit.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await
        .context("failed to parse okx funding history")?;
    let interval = q.interval.as_deref().unwrap_or("8h");
    let interval_ms = interval_to_ms(interval).unwrap_or(28_800_000);
    let rows = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            let open_time_ms = value_u64(row.get("fundingTime")).context("missing fundingTime")?;
            let funding_rate = value_f64(row.get("fundingRate")).context("missing fundingRate")?;
            Ok(KlineBar {
                exchange: "okx".to_string(),
                market: "perp".to_string(),
                symbol: q.symbol.trim().to_ascii_uppercase(),
                interval: interval.to_string(),
                open_time_ms,
                close_time_ms: open_time_ms + interval_ms - 1,
                open: funding_rate,
                high: funding_rate,
                low: funding_rate,
                close: funding_rate,
                volume: None,
                source: "okx_funding_rate_history".to_string(),
                updated_at_ms: now_ms(),
            })
        })
        .collect()
}

fn parse_binance_array_row(
    exchange: &str,
    market: &str,
    symbol: &str,
    interval: &str,
    source: &str,
    row: Vec<Value>,
    has_volume: bool,
) -> Result<KlineBar> {
    if row.len() < 6 {
        bail!("short binance candle row");
    }
    let open_time_ms = value_u64(row.first()).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    let close_time_ms = if has_volume {
        row.get(6)
            .and_then(|value| value_u64(Some(value)))
            .unwrap_or(open_time_ms + interval_ms - 1)
    } else {
        row.get(5)
            .and_then(|value| value_u64(Some(value)))
            .unwrap_or(open_time_ms + interval_ms - 1)
    };
    Ok(KlineBar {
        exchange: exchange.to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms,
        open: value_f64(row.get(1)).context("missing open")?,
        high: value_f64(row.get(2)).context("missing high")?,
        low: value_f64(row.get(3)).context("missing low")?,
        close: value_f64(row.get(4)).context("missing close")?,
        volume: has_volume.then(|| value_f64(row.get(5))).flatten(),
        source: source.to_string(),
        updated_at_ms: now_ms(),
    })
}

fn parse_okx_array_row(
    exchange: &str,
    market: &str,
    symbol: &str,
    interval: &str,
    source: &str,
    row: Value,
    has_volume: bool,
) -> Result<KlineBar> {
    let row = row.as_array().context("okx candle row is not array")?;
    if row.len() < 5 {
        bail!("short okx candle row");
    }
    let open_time_ms = value_u64(row.first()).context("missing open time")?;
    let interval_ms = interval_to_ms(interval).context("unsupported interval")?;
    Ok(KlineBar {
        exchange: exchange.to_string(),
        market: market.to_string(),
        symbol: symbol.to_ascii_uppercase(),
        interval: interval.to_string(),
        open_time_ms,
        close_time_ms: open_time_ms + interval_ms - 1,
        open: value_f64(row.get(1)).context("missing open")?,
        high: value_f64(row.get(2)).context("missing high")?,
        low: value_f64(row.get(3)).context("missing low")?,
        close: value_f64(row.get(4)).context("missing close")?,
        volume: has_volume.then(|| value_f64(row.get(5))).flatten(),
        source: source.to_string(),
        updated_at_ms: now_ms(),
    })
}

fn okx_inst_id(symbol: &str, market: &str) -> String {
    let symbol = symbol.trim().to_ascii_uppercase();
    let (base, quote) = symbol
        .strip_suffix("USDT")
        .map(|base| (base, "USDT"))
        .or_else(|| symbol.strip_suffix("USDC").map(|base| (base, "USDC")))
        .unwrap_or((symbol.as_str(), ""));
    if market == "spot" {
        format!("{base}-{quote}")
    } else {
        format!("{base}-{quote}-SWAP")
    }
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_str()
            .and_then(|x| x.parse().ok())
            .or_else(|| value.as_f64())
    })
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_str()
            .and_then(|x| x.parse().ok())
            .or_else(|| value.as_u64())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_binance_mark_row_without_volume() {
        let row = serde_json::json!([1000, "1.0", "2.0", "0.5", "1.5", 1999])
            .as_array()
            .expect("array")
            .clone();
        let bar = parse_binance_array_row("binance", "perp", "BTCUSDT", "1m", "test", row, false)
            .expect("bar");
        assert_eq!(bar.volume, None);
        assert_eq!(bar.close_time_ms, 1999);
    }

    #[test]
    fn okx_symbol_maps_perp_swap() {
        assert_eq!(okx_inst_id("BTCUSDT", "perp"), "BTC-USDT-SWAP");
        assert_eq!(okx_inst_id("BTCUSDT", "spot"), "BTC-USDT");
    }
}
