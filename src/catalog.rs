use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CatalogSource {
    pub source_type: &'static str,
    pub source: &'static str,
    pub venue: Option<&'static str>,
    pub domains: Vec<&'static str>,
    pub connector_path: &'static str,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogDomain {
    pub domain: &'static str,
    pub endpoint: &'static str,
    pub status: &'static str,
}

pub fn source_catalog() -> Vec<CatalogSource> {
    vec![
        CatalogSource {
            source_type: "exchange",
            source: "cex_adapters",
            venue: None,
            domains: vec![
                "market_quote",
                "market_funding",
                "market_open_interest",
                "market_liquidation",
                "market_order_book",
                "market_trade",
            ],
            connector_path: "src/connectors/cex",
            status: "implemented",
        },
        CatalogSource {
            source_type: "options_venue",
            source: "deribit",
            venue: Some("deribit"),
            domains: vec!["options_chain"],
            connector_path: "src/connectors/options/deribit.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "options_venue",
            source: "okx",
            venue: Some("okx"),
            domains: vec!["options_chain"],
            connector_path: "src/connectors/options/okx.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "options_venue",
            source: "bybit",
            venue: Some("bybit"),
            domains: vec!["options_chain"],
            connector_path: "src/connectors/options/bybit.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "options_venue",
            source: "binance",
            venue: Some("binance"),
            domains: vec!["options_chain"],
            connector_path: "src/connectors/options/binance.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "prediction_market",
            source: "polymarket",
            venue: Some("polymarket"),
            domains: vec!["prediction_market", "prediction_book"],
            connector_path: "src/connectors/prediction/polymarket.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "defi_venue",
            source: "jupiter",
            venue: Some("jupiter"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/defi/jupiter.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "defi_venue",
            source: "raydium",
            venue: Some("raydium"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/defi/raydium.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "defi_venue",
            source: "uniswap_v3",
            venue: Some("uniswap_v3"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/defi/uniswap_v3.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "defi_venue",
            source: "paraswap",
            venue: Some("paraswap"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/defi/paraswap.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "defi_venue",
            source: "oneinch",
            venue: Some("oneinch"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/defi/oneinch.rs",
            status: "implemented",
        },
    ]
}

pub fn domain_catalog() -> Vec<CatalogDomain> {
    vec![
        CatalogDomain {
            domain: "market_quote",
            endpoint: "/v1/market/quotes",
            status: "implemented",
        },
        CatalogDomain {
            domain: "market_funding",
            endpoint: "/v1/market/funding",
            status: "implemented",
        },
        CatalogDomain {
            domain: "market_open_interest",
            endpoint: "/v1/market/open-interest",
            status: "implemented",
        },
        CatalogDomain {
            domain: "market_liquidation",
            endpoint: "/v1/market/liquidations",
            status: "implemented",
        },
        CatalogDomain {
            domain: "market_order_book",
            endpoint: "/v1/market/order-books",
            status: "implemented",
        },
        CatalogDomain {
            domain: "market_trade",
            endpoint: "/v1/market/trades",
            status: "implemented",
        },
        CatalogDomain {
            domain: "options_chain",
            endpoint: "/v1/options/chains",
            status: "implemented",
        },
        CatalogDomain {
            domain: "prediction_book",
            endpoint: "/v1/prediction/books",
            status: "implemented",
        },
    ]
}

pub fn health_status(records: usize, stale_records: usize) -> &'static str {
    if records == 0 {
        "no_data"
    } else if stale_records == 0 {
        "healthy"
    } else if stale_records >= records {
        "stale"
    } else {
        "degraded"
    }
}

#[cfg(test)]
mod tests {
    use super::{domain_catalog, health_status, source_catalog};

    #[test]
    fn catalogs_expose_current_public_domains() {
        let domains = domain_catalog()
            .into_iter()
            .map(|domain| domain.domain)
            .collect::<Vec<_>>();
        assert!(domains.contains(&"market_quote"));
        assert!(domains.contains(&"options_chain"));
        assert!(domains.contains(&"prediction_book"));

        let sources = source_catalog()
            .into_iter()
            .map(|source| source.source)
            .collect::<Vec<_>>();
        assert!(sources.contains(&"cex_adapters"));
        assert!(sources.contains(&"deribit"));
        assert!(sources.contains(&"polymarket"));
    }

    #[test]
    fn health_status_labels_source_freshness() {
        assert_eq!(health_status(0, 0), "no_data");
        assert_eq!(health_status(10, 0), "healthy");
        assert_eq!(health_status(10, 3), "degraded");
        assert_eq!(health_status(10, 10), "stale");
    }
}
