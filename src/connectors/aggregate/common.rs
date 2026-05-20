use anyhow::{Context, Result};

use crate::source::SourceContext;
use crate::types::{DataEvent, ExternalSignalTick, MarketKind, MarketTick, now_ms};

pub fn configured_api_key(explicit: &Option<String>, env_name: &str) -> Option<String> {
    explicit
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var(env_name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

pub fn require_api_key(explicit: &Option<String>, env_name: &str) -> Result<String> {
    configured_api_key(explicit, env_name).with_context(|| format!("missing API key: {env_name}"))
}

pub fn price_to_bid_ask(price: f64, spread_bps: f64) -> Option<(f64, f64)> {
    if price <= 0.0 || !price.is_finite() {
        return None;
    }
    let half_spread = (spread_bps.max(0.0) / 10_000.0) / 2.0;
    Some((price * (1.0 - half_spread), price * (1.0 + half_spread)))
}

pub async fn emit_price_quote(
    ctx: &SourceContext,
    source: &'static str,
    symbol: &str,
    price: f64,
    spread_bps: f64,
) -> Result<()> {
    if let Some((bid, ask)) = price_to_bid_ask(price, spread_bps) {
        ctx.emit(DataEvent::Tick(MarketTick {
            exchange: source,
            market: MarketKind::Spot,
            symbol: symbol.to_ascii_uppercase().into_boxed_str(),
            bid,
            ask,
            mark: Some(price),
            funding_rate: None,
            ts_ms: now_ms(),
        }))
        .await?;
    }
    Ok(())
}

pub async fn emit_external_signal(
    ctx: &SourceContext,
    source: &'static str,
    category: &str,
    symbol: Option<&str>,
    metric: &str,
    value: Option<f64>,
    raw: Option<serde_json::Value>,
) -> Result<()> {
    ctx.emit(DataEvent::ExternalSignal(ExternalSignalTick {
        source,
        category: category.to_string().into_boxed_str(),
        symbol: symbol.map(|x| x.to_ascii_uppercase().into_boxed_str()),
        metric: metric.to_string().into_boxed_str(),
        value,
        score: value,
        title: None,
        url: None,
        ts_ms: now_ms(),
        raw,
    }))
    .await
}

pub fn parse_f64_value(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_numbers_or_strings() {
        assert_eq!(parse_f64_value(&serde_json::json!(1.5)), Some(1.5));
        assert_eq!(parse_f64_value(&serde_json::json!("2.5")), Some(2.5));
        assert_eq!(parse_f64_value(&serde_json::json!(null)), None);
    }
}
