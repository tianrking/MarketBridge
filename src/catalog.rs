use serde::Serialize;

use crate::config::AppConfig;

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
        CatalogSource {
            source_type: "tradfi_reference",
            source: "dxy",
            venue: Some("yahoo_finance"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/tradfi/yahoo.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "tradfi_reference",
            source: "vix",
            venue: Some("yahoo_finance"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/tradfi/yahoo.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "tradfi_reference",
            source: "us10y",
            venue: Some("fred"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/tradfi/fred.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "aggregate_data",
            source: "coingecko",
            venue: Some("coingecko"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/aggregate/coingecko.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "aggregate_data",
            source: "coincap",
            venue: Some("coincap"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/aggregate/coincap.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "aggregate_data",
            source: "coinmarketcap",
            venue: Some("coinmarketcap"),
            domains: vec!["market_quote"],
            connector_path: "src/connectors/aggregate/coinmarketcap.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "aggregate_data",
            source: "coinglass",
            venue: Some("coinglass"),
            domains: vec!["external_signal"],
            connector_path: "src/connectors/aggregate/coinglass.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "sentiment",
            source: "fear_greed",
            venue: Some("alternative_me"),
            domains: vec!["external_signal"],
            connector_path: "src/connectors/sentiment/fear_greed.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "sentiment",
            source: "cryptopanic",
            venue: Some("cryptopanic"),
            domains: vec!["external_signal"],
            connector_path: "src/connectors/sentiment/cryptopanic.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "sentiment",
            source: "santiment",
            venue: Some("santiment"),
            domains: vec!["external_signal"],
            connector_path: "src/connectors/sentiment/santiment.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "sentiment",
            source: "lunarcrush",
            venue: Some("lunarcrush"),
            domains: vec!["external_signal"],
            connector_path: "src/connectors/sentiment/lunarcrush.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "onchain",
            source: "whale_alert",
            venue: Some("whale_alert"),
            domains: vec!["onchain_transfer"],
            connector_path: "src/onchain.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "onchain",
            source: "mempool_space",
            venue: Some("mempool_space"),
            domains: vec!["onchain_transfer"],
            connector_path: "src/onchain.rs",
            status: "implemented",
        },
        CatalogSource {
            source_type: "onchain",
            source: "etherscan",
            venue: Some("etherscan"),
            domains: vec!["onchain_transfer"],
            connector_path: "src/onchain.rs",
            status: "implemented",
        },
    ]
}

pub fn source_catalog_for_config(cfg: &AppConfig) -> Vec<CatalogSource> {
    source_catalog()
        .into_iter()
        .map(|mut source| {
            source.status = source_runtime_status(cfg, source.source);
            source
        })
        .collect()
}

fn source_runtime_status(cfg: &AppConfig, source: &str) -> &'static str {
    match source {
        "cex_adapters" if cfg.exchanges.values().any(|exchange| exchange.enabled) => "enabled",
        "cex_adapters" => "available",
        "deribit" => enabled_status(cfg.deribit.enabled),
        "okx" => enabled_status(cfg.okx_options.enabled),
        "bybit" => enabled_status(cfg.bybit_options.enabled),
        "binance" => enabled_status(cfg.binance_options.enabled),
        "polymarket" => enabled_status(cfg.polymarket.enabled),
        "jupiter" => enabled_status(cfg.defi.jupiter.enabled),
        "raydium" => enabled_status(cfg.defi.raydium.enabled),
        "uniswap_v3" => enabled_status(cfg.defi.uniswap_v3.enabled),
        "paraswap" => enabled_status(cfg.defi.paraswap.enabled),
        "oneinch" => enabled_status(cfg.defi.oneinch.enabled),
        "dxy" => enabled_status(cfg.tradfi.dxy.enabled),
        "vix" => enabled_status(cfg.tradfi.vix.enabled),
        "us10y" => keyed_status(
            cfg.tradfi.us10y.enabled,
            cfg.tradfi.us10y.api_key.as_deref(),
            &cfg.tradfi.us10y.api_key_env,
        ),
        "coingecko" => enabled_status(cfg.aggregates.coingecko.enabled),
        "coincap" => enabled_status(cfg.aggregates.coincap.enabled),
        "coinmarketcap" => keyed_status(
            cfg.aggregates.coinmarketcap.enabled,
            cfg.aggregates.coinmarketcap.api_key.as_deref(),
            &cfg.aggregates.coinmarketcap.api_key_env,
        ),
        "coinglass" => keyed_status(
            cfg.aggregates.coinglass.enabled,
            cfg.aggregates.coinglass.api_key.as_deref(),
            &cfg.aggregates.coinglass.api_key_env,
        ),
        "fear_greed" => enabled_status(cfg.sentiment.fear_greed.enabled),
        "cryptopanic" => keyed_status(
            cfg.sentiment.cryptopanic.enabled,
            cfg.sentiment.cryptopanic.api_key.as_deref(),
            &cfg.sentiment.cryptopanic.api_key_env,
        ),
        "santiment" => keyed_status(
            cfg.sentiment.santiment.enabled,
            cfg.sentiment.santiment.api_key.as_deref(),
            &cfg.sentiment.santiment.api_key_env,
        ),
        "lunarcrush" => keyed_status(
            cfg.sentiment.lunarcrush.enabled,
            cfg.sentiment.lunarcrush.api_key.as_deref(),
            &cfg.sentiment.lunarcrush.api_key_env,
        ),
        "whale_alert" => keyed_status(
            cfg.onchain.whale_alert.enabled,
            cfg.onchain.whale_alert.api_key.as_deref(),
            &cfg.onchain.whale_alert.api_key_env,
        ),
        "mempool_space" => enabled_status(cfg.onchain.mempool_space.enabled),
        "etherscan" => keyed_status(
            cfg.onchain.etherscan.enabled,
            cfg.onchain.etherscan.api_key.as_deref(),
            &cfg.onchain.etherscan.api_key_env,
        ),
        _ => "available",
    }
}

fn enabled_status(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "available" }
}

fn keyed_status(enabled: bool, api_key: Option<&str>, api_key_env: &str) -> &'static str {
    if !enabled {
        return "available";
    }
    let inline_key = api_key.is_some_and(|key| !key.trim().is_empty());
    let env_key = std::env::var(api_key_env).is_ok_and(|key| !key.trim().is_empty());
    if inline_key || env_key {
        "enabled"
    } else {
        "enabled_missing_api_key"
    }
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
        CatalogDomain {
            domain: "external_signal",
            endpoint: "/v1/external/signals",
            status: "implemented",
        },
        CatalogDomain {
            domain: "onchain_transfer",
            endpoint: "/v1/onchain/transfers",
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
        assert!(domains.contains(&"external_signal"));

        let sources = source_catalog()
            .into_iter()
            .map(|source| source.source)
            .collect::<Vec<_>>();
        assert!(sources.contains(&"cex_adapters"));
        assert!(sources.contains(&"deribit"));
        assert!(sources.contains(&"polymarket"));
        assert!(sources.contains(&"coingecko"));
        assert!(sources.contains(&"fear_greed"));
    }

    #[test]
    fn health_status_labels_source_freshness() {
        assert_eq!(health_status(0, 0), "no_data");
        assert_eq!(health_status(10, 0), "healthy");
        assert_eq!(health_status(10, 3), "degraded");
        assert_eq!(health_status(10, 10), "stale");
    }
}
