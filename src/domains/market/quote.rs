use serde::{Deserialize, Serialize};

use crate::core::schema::{
    AssetClass, DataDomain, DataEnvelope, Freshness, InstrumentRef, ProductType, SourceRef,
    SourceType,
};
use crate::event_bus::NormalizedTick;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotePayload {
    pub bid: f64,
    pub ask: f64,
    pub mark: Option<f64>,
    pub funding: Option<f64>,
}

pub fn envelope_from_tick(tick: NormalizedTick) -> DataEnvelope<QuotePayload> {
    let product_type = match tick.market {
        "spot" => ProductType::Spot,
        "perp" => ProductType::Perp,
        _ => ProductType::Spot,
    };
    let (base, quote) = split_symbol(&tick.symbol);
    let instrument_id = instrument_id(&tick.symbol, product_type);

    DataEnvelope::new(
        DataDomain::MarketQuote,
        SourceRef {
            source_type: SourceType::Exchange,
            source: tick.exchange.to_string(),
            venue: Some(tick.exchange.to_string()),
            chain: None,
            protocol: None,
        },
        InstrumentRef {
            asset_class: AssetClass::Crypto,
            product_type,
            instrument_id,
            symbol: Some(tick.symbol.clone()),
            base,
            quote,
            market_id: None,
        },
        Freshness {
            ts_source: tick.ts,
            ts_received: tick.ts.saturating_add(tick.source_latency_ms),
            latency_ms: tick.source_latency_ms,
            stale: tick.stale,
        },
        QuotePayload {
            bid: tick.bid,
            ask: tick.ask,
            mark: tick.mark,
            funding: tick.funding,
        },
    )
}

fn instrument_id(symbol: &str, product_type: ProductType) -> String {
    let product = match product_type {
        ProductType::Spot => "SPOT",
        ProductType::Perp => "PERP",
        _ => "MARKET",
    };
    let (base, quote) = split_symbol(symbol);
    match (base, quote) {
        (Some(base), Some(quote)) => format!("{base}-{quote}-{product}"),
        _ => format!("{}-{product}", symbol.to_ascii_uppercase()),
    }
}

fn split_symbol(symbol: &str) -> (Option<String>, Option<String>) {
    let clean = symbol
        .trim()
        .trim_start_matches('t')
        .replace(['-', '/', '_', ':'], "");
    for quote in ["USDT", "USDC", "USD", "BTC", "ETH"] {
        if let Some(base) = clean.strip_suffix(quote)
            && !base.is_empty()
        {
            return (Some(base.to_ascii_uppercase()), Some(quote.to_string()));
        }
    }
    (Some(clean.to_ascii_uppercase()), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tick_to_quote_envelope() {
        let envelope = envelope_from_tick(NormalizedTick {
            version: "v1",
            exchange: "okx",
            market: "perp",
            symbol: "BTCUSDT".to_string(),
            bid: 99.0,
            ask: 100.0,
            mark: Some(100.0),
            funding: Some(0.0001),
            ts: 1_000,
            source_latency_ms: 3,
            stale: false,
        });

        assert_eq!(envelope.domain, DataDomain::MarketQuote);
        assert_eq!(envelope.source_ref.source, "okx");
        assert_eq!(envelope.instrument_ref.product_type, ProductType::Perp);
        assert_eq!(envelope.instrument_ref.instrument_id, "BTC-USDT-PERP");
        assert_eq!(envelope.payload.ask, 100.0);
    }
}
