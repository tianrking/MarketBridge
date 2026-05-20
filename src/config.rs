use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::types::BackpressureMode;

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
    pub symbols: Vec<String>,
    pub perp_symbols: Option<Vec<String>>,
    pub exchanges: HashMap<String, ExchangeConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub queue_capacity: usize,
    pub backpressure: BackpressureConfig,
    pub report_interval_ms: u64,
    pub stale_ttl_ms: u64,
    #[serde(default = "default_api_addr")]
    pub api_addr: String,
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default = "default_redis_stream_prefix")]
    pub redis_stream_prefix: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressureConfig {
    Block,
    DropNewest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub min_profit_usdt: f64,
    pub min_profit_bps: f64,
    pub min_signal_hold_ms: u64,
    pub slippage_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeribitConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_deribit_base_url")]
    pub base_url: String,
    #[serde(default = "default_deribit_currencies")]
    pub currencies: Vec<String>,
    #[serde(default = "default_deribit_refresh_secs")]
    pub refresh_secs: u64,
    #[serde(default = "default_deribit_stale_ttl_ms")]
    pub stale_ttl_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OkxOptionsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_okx_options_base_url")]
    pub base_url: String,
    #[serde(default = "default_deribit_currencies")]
    pub currencies: Vec<String>,
    #[serde(default = "default_deribit_refresh_secs")]
    pub refresh_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BybitOptionsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_bybit_options_base_url")]
    pub base_url: String,
    #[serde(default = "default_deribit_currencies")]
    pub currencies: Vec<String>,
    #[serde(default = "default_deribit_refresh_secs")]
    pub refresh_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceOptionsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_binance_options_base_url")]
    pub base_url: String,
    #[serde(default = "default_deribit_currencies")]
    pub currencies: Vec<String>,
    #[serde(default = "default_deribit_refresh_secs")]
    pub refresh_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolymarketConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_polymarket_ws_url")]
    pub ws_url: String,
    #[serde(default = "default_polymarket_gamma_base_url")]
    pub gamma_base_url: String,
    #[serde(default = "default_polymarket_limit")]
    pub limit: usize,
    #[serde(default = "default_polymarket_max_offset")]
    pub max_offset: usize,
    #[serde(default = "default_polymarket_refresh_secs")]
    pub refresh_secs: u64,
    #[serde(default = "default_polymarket_ping_secs")]
    pub ping_secs: u64,
    #[serde(default = "default_polymarket_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_polymarket_stale_ttl_ms")]
    pub stale_ttl_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExchangeConfig {
    pub enabled: bool,
    pub symbols: Option<Vec<String>>,      // spot symbols override
    pub perp_symbols: Option<Vec<String>>, // perp symbols override
    pub fee: FeeModel,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FeeModel {
    Fixed {
        #[serde(rename = "maker_bps")]
        _maker_bps: f64,
        taker_bps: f64,
    },
    Tiered {
        volume_30d_usdt: f64,
        tiers: Vec<FeeTier>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeeTier {
    pub min_volume_usdt: f64,
    #[serde(rename = "maker_bps")]
    pub _maker_bps: f64,
    pub taker_bps: f64,
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

        for ex in cfg.exchanges.values_mut() {
            if let Some(symbols) = &mut ex.symbols {
                *symbols = normalize_symbols(symbols);
            }
            if let Some(perp) = &mut ex.perp_symbols {
                *perp = normalize_symbols(perp);
            }
        }

        Ok(cfg)
    }

    pub fn backpressure_mode(&self) -> BackpressureMode {
        match self.runtime.backpressure {
            BackpressureConfig::Block => BackpressureMode::Block,
            BackpressureConfig::DropNewest => BackpressureMode::DropNewest,
        }
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
}

impl FeeModel {
    pub fn taker_bps(&self) -> f64 {
        match self {
            FeeModel::Fixed { taker_bps, .. } => *taker_bps,
            FeeModel::Tiered {
                volume_30d_usdt,
                tiers,
            } => select_fee_tier(*volume_30d_usdt, tiers)
                .map(|x| x.taker_bps)
                .unwrap_or(0.0),
        }
    }
}

fn select_fee_tier(volume_30d_usdt: f64, tiers: &[FeeTier]) -> Option<&FeeTier> {
    let mut best: Option<&FeeTier> = None;
    for tier in tiers {
        if volume_30d_usdt >= tier.min_volume_usdt
            && best.is_none_or(|x| tier.min_volume_usdt > x.min_volume_usdt)
        {
            best = Some(tier);
        }
    }
    best.or_else(|| tiers.first())
}

fn normalize_symbols(input: &[String]) -> Vec<String> {
    input
        .iter()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn default_api_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_redis_stream_prefix() -> String {
    "ticks".to_string()
}

fn default_deribit_base_url() -> String {
    "https://www.deribit.com/api/v2/".to_string()
}

fn default_deribit_currencies() -> Vec<String> {
    vec!["BTC".to_string(), "ETH".to_string()]
}

fn default_deribit_refresh_secs() -> u64 {
    10
}

fn default_deribit_stale_ttl_ms() -> u64 {
    30_000
}

fn default_okx_options_base_url() -> String {
    "https://www.okx.com/api/v5/".to_string()
}

fn default_bybit_options_base_url() -> String {
    "https://api.bybit.com/v5/".to_string()
}

fn default_binance_options_base_url() -> String {
    "https://eapi.binance.com/".to_string()
}

fn default_polymarket_ws_url() -> String {
    "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string()
}

fn default_polymarket_gamma_base_url() -> String {
    "https://gamma-api.polymarket.com/".to_string()
}

fn default_polymarket_limit() -> usize {
    500
}

fn default_polymarket_max_offset() -> usize {
    5000
}

fn default_polymarket_refresh_secs() -> u64 {
    300
}

fn default_polymarket_ping_secs() -> u64 {
    10
}

fn default_polymarket_chunk_size() -> usize {
    500
}

fn default_polymarket_stale_ttl_ms() -> u64 {
    1500
}

impl Default for PolymarketConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ws_url: default_polymarket_ws_url(),
            gamma_base_url: default_polymarket_gamma_base_url(),
            limit: default_polymarket_limit(),
            max_offset: default_polymarket_max_offset(),
            refresh_secs: default_polymarket_refresh_secs(),
            ping_secs: default_polymarket_ping_secs(),
            chunk_size: default_polymarket_chunk_size(),
            stale_ttl_ms: default_polymarket_stale_ttl_ms(),
        }
    }
}

impl Default for DeribitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_deribit_base_url(),
            currencies: default_deribit_currencies(),
            refresh_secs: default_deribit_refresh_secs(),
            stale_ttl_ms: default_deribit_stale_ttl_ms(),
        }
    }
}

impl Default for OkxOptionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_okx_options_base_url(),
            currencies: default_deribit_currencies(),
            refresh_secs: default_deribit_refresh_secs(),
        }
    }
}

impl Default for BybitOptionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_bybit_options_base_url(),
            currencies: default_deribit_currencies(),
            refresh_secs: default_deribit_refresh_secs(),
        }
    }
}

impl Default for BinanceOptionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_binance_options_base_url(),
            currencies: default_deribit_currencies(),
            refresh_secs: default_deribit_refresh_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiered_fee_selects_highest_matching_tier() {
        let f = FeeModel::Tiered {
            volume_30d_usdt: 5_500_000.0,
            tiers: vec![
                FeeTier {
                    min_volume_usdt: 0.0,
                    _maker_bps: 10.0,
                    taker_bps: 12.0,
                },
                FeeTier {
                    min_volume_usdt: 1_000_000.0,
                    _maker_bps: 8.0,
                    taker_bps: 9.0,
                },
                FeeTier {
                    min_volume_usdt: 5_000_000.0,
                    _maker_bps: 6.0,
                    taker_bps: 7.0,
                },
            ],
        };
        assert!((f.taker_bps() - 7.0).abs() < 1e-9);
    }
}
