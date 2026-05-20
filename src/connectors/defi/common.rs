use anyhow::Result;

use crate::source::SourceContext;
use crate::types::{DataEvent, MarketKind, MarketTick, now_ms};

pub fn quote_to_price(
    in_amount: f64,
    out_amount: f64,
    input_decimals: u8,
    output_decimals: u8,
) -> Option<f64> {
    if in_amount <= 0.0 || out_amount <= 0.0 {
        return None;
    }
    let base_units = 10_f64.powi(input_decimals as i32);
    let quote_units = 10_f64.powi(output_decimals as i32);
    Some((out_amount / quote_units) / (in_amount / base_units))
}

pub fn price_to_bid_ask(price: f64, spread_bps: f64) -> Option<(f64, f64)> {
    if price <= 0.0 || !price.is_finite() {
        return None;
    }
    let half_spread = (spread_bps.max(0.0) / 10_000.0) / 2.0;
    Some((price * (1.0 - half_spread), price * (1.0 + half_spread)))
}

pub async fn emit_defi_quote(
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
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_price_uses_token_decimals() {
        let price = quote_to_price(1_000_000_000.0, 200_000_000.0, 9, 6).unwrap();
        assert_eq!(price, 200.0);
    }

    #[test]
    fn synthetic_spread_wraps_mid_price() {
        let (bid, ask) = price_to_bid_ask(100.0, 10.0).unwrap();
        assert_eq!(bid, 99.95);
        assert_eq!(ask, 100.05);
    }
}
