#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::types::now_ms;

pub const ENVELOPE_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Exchange,
    OptionsVenue,
    PredictionMarket,
    Onchain,
    ExternalEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataDomain {
    MarketQuote,
    MarketOrderBook,
    MarketTrade,
    MarketFunding,
    OptionsChain,
    PredictionMarket,
    PredictionBook,
    OnchainTransfer,
    OnchainDex,
    ExternalEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Crypto,
    Prediction,
    Equity,
    Rates,
    Commodity,
    Fx,
    Weather,
    News,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductType {
    Spot,
    Perp,
    Future,
    Option,
    BinaryOutcome,
    WalletTransfer,
    DexPool,
    Event,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub source_type: SourceType,
    pub source: String,
    pub venue: Option<String>,
    pub chain: Option<String>,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentRef {
    pub asset_class: AssetClass,
    pub product_type: ProductType,
    pub instrument_id: String,
    pub symbol: Option<String>,
    pub base: Option<String>,
    pub quote: Option<String>,
    pub market_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Freshness {
    pub ts_source: u64,
    pub ts_received: u64,
    pub latency_ms: u64,
    pub stale: bool,
}

impl Freshness {
    pub fn from_source_ts(ts_source: u64, stale_ttl_ms: u64) -> Self {
        let ts_received = now_ms();
        let latency_ms = ts_received.saturating_sub(ts_source);
        Self {
            ts_source,
            ts_received,
            latency_ms,
            stale: latency_ms > stale_ttl_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEnvelope<T> {
    pub version: &'static str,
    pub domain: DataDomain,
    pub source_ref: SourceRef,
    pub instrument_ref: InstrumentRef,
    pub freshness: Freshness,
    pub payload: T,
}

impl<T> DataEnvelope<T> {
    pub fn new(
        domain: DataDomain,
        source_ref: SourceRef,
        instrument_ref: InstrumentRef,
        freshness: Freshness,
        payload: T,
    ) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            domain,
            source_ref,
            instrument_ref,
            freshness,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct QuotePayload {
        bid: f64,
        ask: f64,
    }

    #[test]
    fn builds_data_envelope() {
        let envelope = DataEnvelope::new(
            DataDomain::MarketQuote,
            SourceRef {
                source_type: SourceType::Exchange,
                source: "binance".to_string(),
                venue: Some("binance".to_string()),
                chain: None,
                protocol: None,
            },
            InstrumentRef {
                asset_class: AssetClass::Crypto,
                product_type: ProductType::Spot,
                instrument_id: "BTC-USDT-SPOT".to_string(),
                symbol: Some("BTCUSDT".to_string()),
                base: Some("BTC".to_string()),
                quote: Some("USDT".to_string()),
                market_id: None,
            },
            Freshness::from_source_ts(now_ms(), 1_000),
            QuotePayload { bid: 1.0, ask: 2.0 },
        );

        assert_eq!(envelope.version, "v1");
        assert_eq!(envelope.domain, DataDomain::MarketQuote);
        assert_eq!(envelope.source_ref.source, "binance");
        assert!(!envelope.freshness.stale);
    }
}
