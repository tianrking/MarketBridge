use serde::Deserialize;

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
