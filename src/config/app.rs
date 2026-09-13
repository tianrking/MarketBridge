use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::types::BackpressureMode;

use super::{
    AggregatesConfig, BinanceOptionsConfig, BybitOptionsConfig, DefiConfig, DeribitConfig,
    ExchangeConfig, KlineConfig, OkxOptionsConfig, OnchainConfig, PolymarketConfig,
    ReferenceDataConfig, RuntimeConfig, SentimentConfig, StrategyConfig, TradfiConfig,
};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub deribit: DeribitConfig,
    #[serde(default)]
    pub okx_options: OkxOptionsConfig,
    #[serde(default)]
    pub bybit_options: BybitOptionsConfig,
    #[serde(default)]
    pub binance_options: BinanceOptionsConfig,
    #[serde(default)]
    pub polymarket: PolymarketConfig,
    #[serde(default)]
    pub defi: DefiConfig,
    #[serde(default)]
    pub tradfi: TradfiConfig,
    #[serde(default)]
    pub aggregates: AggregatesConfig,
    #[serde(default)]
    pub sentiment: SentimentConfig,
    #[serde(default)]
    pub klines: KlineConfig,
    #[serde(default)]
    pub onchain: OnchainConfig,
    #[serde(default)]
    pub reference_data: ReferenceDataConfig,
    pub symbols: Vec<String>,
    pub perp_symbols: Option<Vec<String>>,
    pub exchanges: HashMap<String, ExchangeConfig>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path =
            std::env::var("MARKETBRIDGE_CONFIG").unwrap_or_else(|_| "config.yaml".to_string());
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file: {path}"))?;
        let mut cfg: AppConfig =
            serde_yaml::from_str(&content).with_context(|| format!("invalid yaml: {path}"))?;

        cfg.symbols = normalize_symbols(&cfg.symbols);
        cfg.perp_symbols = cfg.perp_symbols.take().map(|v| normalize_symbols(&v));
        if let Ok(addr) = std::env::var("MARKETBRIDGE_API_ADDR") {
            cfg.runtime.api_addr = addr;
        }

        for ex in cfg.exchanges.values_mut() {
            if let Some(symbols) = &mut ex.symbols {
                *symbols = normalize_symbols(symbols);
            }
            if let Some(perp) = &mut ex.perp_symbols {
                *perp = normalize_symbols(perp);
            }
        }

        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        self.runtime
            .api_addr
            .parse::<std::net::SocketAddr>()
            .context("runtime.api_addr must be a socket address")?;
        ensure!(
            self.runtime.queue_capacity > 0,
            "runtime.queue_capacity must be positive"
        );
        ensure!(
            self.runtime.broadcast_capacity > 0,
            "runtime.broadcast_capacity must be positive"
        );
        ensure!(
            self.runtime.stale_ttl_ms > 0,
            "runtime.stale_ttl_ms must be positive"
        );
        ensure!(
            self.runtime.report_interval_ms > 0,
            "runtime.report_interval_ms must be positive"
        );
        ensure!(
            self.strategy.book_signal_notional_usdt.is_finite()
                && self.strategy.book_signal_notional_usdt > 0.0,
            "book signal notional must be finite and positive"
        );
        ensure!(
            self.strategy.slippage_bps.is_finite() && self.strategy.slippage_bps >= 0.0,
            "slippage_bps must be finite and nonnegative"
        );
        for value in [
            self.strategy.min_profit_usdt,
            self.strategy.min_profit_bps,
            self.strategy.fallback_maker_fee_bps,
            self.strategy.fallback_taker_fee_bps,
        ] {
            ensure!(
                value.is_finite(),
                "strategy thresholds and fees must be finite"
            );
        }
        for (name, exchange) in &self.exchanges {
            if exchange.enabled {
                exchange
                    .fee
                    .validate()
                    .with_context(|| format!("invalid fees for {name}"))?;
            }
        }
        let supply = &self.reference_data.supply;
        if supply.enabled {
            ensure!(
                supply.provider.eq_ignore_ascii_case("coingecko"),
                "reference_data.supply currently supports provider=coingecko only"
            );
            let url = url::Url::parse(&supply.base_url)
                .context("invalid reference_data.supply.base_url")?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "reference_data.supply requires a credential-free HTTP(S) base URL"
            );
            ensure!(
                (10..=86_400).contains(&supply.poll_secs),
                "reference_data.supply.poll_secs must be within 10..86400"
            );
            ensure!(
                !supply.assets.is_empty() && supply.assets.len() <= 512,
                "enabled reference_data.supply requires 1..512 explicit assets"
            );
            let mut asset_ids = std::collections::HashSet::new();
            let mut symbols = std::collections::HashSet::new();
            for asset in &supply.assets {
                ensure!(
                    crate::research_store::valid_id(&asset.asset_id)
                        && !asset.provider_asset_id.trim().is_empty()
                        && !asset.identity_evidence.trim().is_empty()
                        && !asset.perp_symbols.is_empty(),
                    "each supply asset requires a stable ID, provider ID, evidence and explicit perp symbols"
                );
                ensure!(
                    asset_ids.insert(&asset.asset_id),
                    "duplicate supply asset_id"
                );
                for symbol in &asset.perp_symbols {
                    ensure!(
                        !symbol.trim().is_empty() && symbols.insert(symbol.to_ascii_uppercase()),
                        "supply perp symbols must be nonempty and uniquely mapped"
                    );
                }
            }
        }
        let mut quota_names: std::collections::HashMap<&str, _> = std::collections::HashMap::new();
        for quota in &self.aggregates.provider_quotas {
            ensure!(
                !quota.name.trim().is_empty()
                    && quota_names.insert(quota.name.as_str(), quota).is_none(),
                "provider quota names must be nonempty and unique"
            );
            ensure!(
                (1..=1_000_000).contains(&quota.max_requests),
                "provider quota max_requests must be within 1..1000000"
            );
            ensure!(
                (1..=86_400).contains(&quota.window_secs),
                "provider quota window_secs must be within 1..86400"
            );
        }
        let mut names = std::collections::HashSet::new();
        for source in self.aggregates.custom_apis.iter().filter(|s| s.enabled) {
            ensure!(
                !source.name.trim().is_empty() && names.insert(&source.name),
                "enabled custom API names must be nonempty and unique"
            );
            ensure!(
                (1..=86400).contains(&source.poll_secs),
                "custom API poll_secs must be within 1..86400"
            );
            let url = url::Url::parse(&source.url).context("invalid custom API URL")?;
            ensure!(
                matches!(url.scheme(), "http" | "https"),
                "custom APIs require HTTP(S)"
            );
            ensure!(
                url.username().is_empty() && url.password().is_none(),
                "custom API credentials must not appear in URLs"
            );
            ensure!(
                source.quota_weight > 0,
                "custom API quota_weight must be positive"
            );
            if let Some(group) = source.quota_group.as_deref() {
                let quota = quota_names.get(group).with_context(|| {
                    format!("custom API quota_group {group:?} is not configured")
                })?;
                ensure!(
                    source.quota_weight <= quota.max_requests,
                    "custom API quota_weight cannot exceed its provider quota max_requests"
                );
            }
        }
        if self.aggregates.farside_etf.enabled {
            let url = url::Url::parse(&self.aggregates.farside_etf.url)
                .context("invalid aggregates.farside_etf.url")?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "aggregates.farside_etf requires a credential-free HTTP(S) URL"
            );
            ensure!(
                (60..=86_400).contains(&self.aggregates.farside_etf.poll_secs),
                "aggregates.farside_etf.poll_secs must be within 60..86400"
            );
            ensure!(
                !self.aggregates.farside_etf.asset.trim().is_empty(),
                "aggregates.farside_etf.asset must be nonempty"
            );
        }
        Ok(())
    }

    pub fn backpressure_mode(&self) -> BackpressureMode {
        self.runtime.backpressure.mode()
    }

    pub fn symbols_for_exchange(&self, ex: &str) -> Vec<String> {
        let Some(cfg) = self.exchanges.get(ex) else {
            return Vec::new();
        };
        cfg.symbols.clone().unwrap_or_else(|| self.symbols.clone())
    }

    pub fn perp_symbols_for_exchange(&self, ex: &str) -> Vec<String> {
        let Some(cfg) = self.exchanges.get(ex) else {
            return Vec::new();
        };
        if let Some(v) = &cfg.perp_symbols {
            return v.clone();
        }
        self.perp_symbols.clone().unwrap_or_default()
    }

    pub fn enabled_exchanges(&self) -> Vec<String> {
        self.exchanges
            .iter()
            .filter_map(|(k, v)| if v.enabled { Some(k.clone()) } else { None })
            .collect()
    }

    pub fn taker_bps(&self, exchange: &str) -> Option<f64> {
        let ex = self.exchanges.get(exchange)?;
        Some(ex.fee.taker_bps())
    }

    pub fn maker_bps(&self, exchange: &str) -> Option<f64> {
        let ex = self.exchanges.get(exchange)?;
        Some(ex.fee.maker_bps())
    }
}

fn normalize_symbols(input: &[String]) -> Vec<String> {
    input
        .iter()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CustomApiConfig, ProviderQuotaConfig};

    fn base_config() -> AppConfig {
        serde_yaml::from_str(include_str!("../../config.research.yaml"))
            .expect("research fixture config must parse")
    }

    fn custom_source(group: Option<&str>, weight: u32) -> CustomApiConfig {
        CustomApiConfig {
            enabled: true,
            name: "reference-price".to_string(),
            url: "https://example.invalid/price".to_string(),
            category: "reference".to_string(),
            symbol: Some("XAUUSD".to_string()),
            metric: "price".to_string(),
            value_path: "price".to_string(),
            timestamp_path: None,
            timestamp_in_seconds: false,
            poll_secs: 30,
            quota_group: group.map(str::to_string),
            quota_weight: weight,
        }
    }

    #[test]
    fn custom_api_shared_quota_requires_a_declared_group_and_valid_weight() {
        let mut cfg = base_config();
        cfg.aggregates.provider_quotas = vec![ProviderQuotaConfig {
            name: "reference".to_string(),
            max_requests: 10,
            window_secs: 60,
        }];
        cfg.aggregates.custom_apis = vec![custom_source(Some("reference"), 2)];
        cfg.validate().expect("declared shared quota is valid");

        cfg.aggregates.custom_apis[0].quota_group = Some("missing".to_string());
        assert!(cfg.validate().is_err());

        cfg.aggregates.custom_apis[0].quota_group = Some("reference".to_string());
        cfg.aggregates.custom_apis[0].quota_weight = 11;
        assert!(cfg.validate().is_err());
    }
}
