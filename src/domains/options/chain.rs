use crate::connectors::options::common::OptionSummary;
use crate::core::schema::{
    AssetClass, DataDomain, DataEnvelope, Freshness, InstrumentRef, ProductType, SourceRef,
    SourceType,
};
use crate::deribit_cache::CachedOptionSummary;

pub fn envelope_from_option_summary(row: CachedOptionSummary) -> DataEnvelope<OptionSummary> {
    let summary = row.summary;
    let venue = summary.venue.clone();
    let instrument_id = summary.instrument_name.clone();
    let symbol = Some(summary.instrument_name.clone());
    let base = Some(summary.currency.clone());

    DataEnvelope::new(
        DataDomain::OptionsChain,
        SourceRef {
            source_type: SourceType::OptionsVenue,
            source: venue.clone(),
            venue: Some(venue),
            chain: None,
            protocol: None,
        },
        InstrumentRef {
            asset_class: AssetClass::Crypto,
            product_type: ProductType::Option,
            instrument_id,
            symbol,
            base,
            quote: Some("USD".to_string()),
            market_id: None,
        },
        Freshness {
            ts_source: row.received_at_ms,
            ts_received: row.received_at_ms,
            latency_ms: 0,
            stale: row.stale,
        },
        summary,
    )
}

pub fn envelope_from_deribit_summary(row: CachedOptionSummary) -> DataEnvelope<OptionSummary> {
    envelope_from_option_summary(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_deribit_summary_to_envelope() {
        let envelope = envelope_from_option_summary(CachedOptionSummary {
            version: "v1",
            source: "deribit_rest_cache".to_string(),
            received_at_ms: 1_000,
            stale: false,
            summary: OptionSummary {
                venue: "deribit".to_string(),
                currency: "BTC".to_string(),
                instrument_name: "BTC-25DEC26-100000-C".to_string(),
                option_type: Some("call".to_string()),
                strike: Some(100_000.0),
                expiry_time: Some("2026-12-25T08:00:00Z".to_string()),
                bid_price: Some(0.1),
                ask_price: Some(0.2),
                mark_price: Some(0.15),
                mark_iv: Some(50.0),
                delta: None,
                gamma: None,
                theta: None,
                vega: None,
                underlying_price: Some(90_000.0),
                underlying_index: Some("BTC-25DEC26".to_string()),
                open_interest: Some(1.0),
            },
        });

        assert_eq!(envelope.domain, DataDomain::OptionsChain);
        assert_eq!(envelope.source_ref.source, "deribit");
        assert_eq!(
            envelope.instrument_ref.instrument_id,
            "BTC-25DEC26-100000-C"
        );
        assert_eq!(envelope.payload.currency, "BTC");
    }
}
