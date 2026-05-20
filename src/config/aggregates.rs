use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AggregatesConfig {
    #[serde(default)]
    pub coingecko: CoinGeckoConfig,
    #[serde(default)]
    pub coincap: CoinCapConfig,
    #[serde(default)]
    pub coinmarketcap: CoinMarketCapConfig,
    #[serde(default)]
    pub coinglass: CoinGlassConfig,
    #[serde(default)]
    pub custom_apis: Vec<CustomApiConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinGeckoConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_coingecko_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_coingecko_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_aggregate_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_coin_assets")]
    pub assets: Vec<CoinPriceAsset>,
    #[serde(default = "default_aggregate_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinCapConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_coincap_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_coincap_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_aggregate_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_coin_assets")]
    pub assets: Vec<CoinPriceAsset>,
    #[serde(default = "default_aggregate_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinMarketCapConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_coinmarketcap_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_coinmarketcap_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_aggregate_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_coinmarketcap_symbols")]
    pub symbols: Vec<String>,
    #[serde(default = "default_aggregate_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinGlassConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_coinglass_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_coinglass_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_coinglass_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_coinglass_symbols")]
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomApiConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    pub url: String,
    #[serde(default = "default_external_category")]
    pub category: String,
    #[serde(default)]
    pub symbol: Option<String>,
    pub metric: String,
    #[serde(default)]
    pub value_path: String,
    #[serde(default = "default_custom_api_poll_secs")]
    pub poll_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinPriceAsset {
    pub symbol: String,
    pub id: String,
    #[serde(default = "default_vs_currency")]
    pub vs_currency: String,
}
fn default_coingecko_base_url() -> String {
    "https://api.coingecko.com/api/v3/".to_string()
}

fn default_coincap_base_url() -> String {
    "https://api.coincap.io/v2/".to_string()
}

fn default_coinmarketcap_base_url() -> String {
    "https://pro-api.coinmarketcap.com/v2/".to_string()
}

fn default_coinglass_base_url() -> String {
    "https://open-api-v4.coinglass.com/".to_string()
}

fn default_coingecko_api_key_env() -> String {
    "COINGECKO_API_KEY".to_string()
}

fn default_coincap_api_key_env() -> String {
    "COINCAP_API_KEY".to_string()
}

fn default_coinmarketcap_api_key_env() -> String {
    "COINMARKETCAP_API_KEY".to_string()
}

fn default_coinglass_api_key_env() -> String {
    "COINGLASS_API_KEY".to_string()
}

fn default_aggregate_poll_secs() -> u64 {
    60
}

fn default_coinglass_poll_secs() -> u64 {
    60
}

fn default_custom_api_poll_secs() -> u64 {
    5
}

fn default_aggregate_spread_bps() -> f64 {
    2.0
}

fn default_external_category() -> String {
    "custom".to_string()
}

fn default_vs_currency() -> String {
    "usd".to_string()
}

fn default_coin_assets() -> Vec<CoinPriceAsset> {
    vec![
        CoinPriceAsset {
            symbol: "BTCUSD".to_string(),
            id: "bitcoin".to_string(),
            vs_currency: default_vs_currency(),
        },
        CoinPriceAsset {
            symbol: "ETHUSD".to_string(),
            id: "ethereum".to_string(),
            vs_currency: default_vs_currency(),
        },
    ]
}

fn default_coinmarketcap_symbols() -> Vec<String> {
    vec!["BTC".to_string(), "ETH".to_string()]
}

fn default_coinglass_symbols() -> Vec<String> {
    vec!["BTC".to_string(), "ETH".to_string()]
}
impl Default for CoinGeckoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_coingecko_base_url(),
            api_key: None,
            api_key_env: default_coingecko_api_key_env(),
            poll_secs: default_aggregate_poll_secs(),
            assets: default_coin_assets(),
            spread_bps: default_aggregate_spread_bps(),
        }
    }
}

impl Default for CoinCapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_coincap_base_url(),
            api_key: None,
            api_key_env: default_coincap_api_key_env(),
            poll_secs: default_aggregate_poll_secs(),
            assets: default_coin_assets(),
            spread_bps: default_aggregate_spread_bps(),
        }
    }
}

impl Default for CoinMarketCapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_coinmarketcap_base_url(),
            api_key: None,
            api_key_env: default_coinmarketcap_api_key_env(),
            poll_secs: default_aggregate_poll_secs(),
            symbols: default_coinmarketcap_symbols(),
            spread_bps: default_aggregate_spread_bps(),
        }
    }
}

impl Default for CoinGlassConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_coinglass_base_url(),
            api_key: None,
            api_key_env: default_coinglass_api_key_env(),
            poll_secs: default_coinglass_poll_secs(),
            symbols: default_coinglass_symbols(),
        }
    }
}
