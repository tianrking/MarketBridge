use anyhow::Result;

use crate::source::SourceContext;
use crate::types::{DataEvent, MarketKind, MarketTick, now_ms};

pub fn price_to_bid_ask(price: f64, spread_bps: f64) -> Option<(f64, f64)> {
    if price <= 0.0 || !price.is_finite() {
        return None;
    }
    let half_spread = (spread_bps.max(0.0) / 10_000.0) / 2.0;
    Some((price * (1.0 - half_spread), price * (1.0 + half_spread)))
}

pub async fn emit_tradfi_quote(
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

pub fn parse_f64_str(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() || text == "." {
        return None;
    }
    text.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tradfi_spread_wraps_mid_price() {
        let (bid, ask) = price_to_bid_ask(100.0, 2.0).unwrap();
        assert_eq!(bid, 99.99);
        assert_eq!(ask, 100.01);
    }

    #[test]
    fn fred_missing_value_is_rejected() {
        assert_eq!(parse_f64_str("."), None);
    }
}
