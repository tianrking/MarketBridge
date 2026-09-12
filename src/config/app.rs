use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::types::BackpressureMode;

use super::{
    AggregatesConfig, BinanceOptionsConfig, BybitOptionsConfig, DefiConfig, DeribitConfig,
    ExchangeConfig, KlineConfig, OkxOptionsConfig, OnchainConfig, PolymarketConfig, RuntimeConfig,
    SentimentConfig, StrategyConfig, TradfiConfig,
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
