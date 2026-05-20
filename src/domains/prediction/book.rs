use crate::core::schema::{
    AssetClass, DataDomain, DataEnvelope, Freshness, InstrumentRef, ProductType, SourceRef,
    SourceType,
};
use crate::polymarket_ws::CachedPolymarketBook;

pub fn envelope_from_polymarket_book(
    book: CachedPolymarketBook,
) -> DataEnvelope<CachedPolymarketBook> {
    let instrument_id = format!(
        "polymarket:{}:{}",
        book.market.as_deref().unwrap_or("unknown"),
        book.asset_id
    );
    let market_id = book.market.clone();

    DataEnvelope::new(
        DataDomain::PredictionBook,
        SourceRef {
            source_type: SourceType::PredictionMarket,
            source: "polymarket".to_string(),
            venue: Some("polymarket".to_string()),
            chain: Some("polygon".to_string()),
            protocol: Some("clob".to_string()),
        },
        InstrumentRef {
            asset_class: AssetClass::Prediction,
            product_type: ProductType::BinaryOutcome,
            instrument_id,
            symbol: Some(book.asset_id.clone()),
            base: None,
            quote: Some("USDC".to_string()),
            market_id,
        },
        Freshness {
            ts_source: book
                .timestamp
                .as_deref()
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or(book.received_at_ms),
            ts_received: book.received_at_ms,
            latency_ms: book.source_latency_ms.unwrap_or(0),
            stale: book.stale,
        },
        book,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_polymarket_book_to_envelope() {
        let envelope = envelope_from_polymarket_book(CachedPolymarketBook {
            version: "v1",
            source: "polymarket_clob_ws",
            market: Some("condition".to_string()),
            asset_id: "token".to_string(),
            timestamp: Some("1000".to_string()),
            best_bid: Some(0.49),
            best_ask: Some(0.51),
            spread: Some(0.02),
            bid_depth: Some(100.0),
            ask_depth: Some(120.0),
            raw_bid_levels: Some(3),
            raw_ask_levels: Some(4),
            last_event_type: "book".to_string(),
            received_at_ms: 1_010,
            source_latency_ms: Some(10),
            stale: false,
        });

        assert_eq!(envelope.domain, DataDomain::PredictionBook);
        assert_eq!(envelope.source_ref.source, "polymarket");
        assert_eq!(
            envelope.instrument_ref.product_type,
            ProductType::BinaryOutcome
        );
        assert_eq!(envelope.payload.asset_id, "token");
    }
}
