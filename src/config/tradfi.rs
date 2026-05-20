use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TradfiConfig {
    #[serde(default)]
    pub dxy: YahooIndicatorConfig,
    #[serde(default)]
    pub vix: YahooIndicatorConfig,
    #[serde(default)]
    pub us10y: FredSeriesConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YahooIndicatorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_yahoo_base_url")]
    pub base_url: String,
    #[serde(default = "default_dxy_yahoo_symbol")]
    pub yahoo_symbol: String,
    #[serde(default = "default_dxy_symbol")]
    pub symbol: String,
    #[serde(default = "default_tradfi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_tradfi_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FredSeriesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_fred_base_url")]
    pub base_url: String,
    #[serde(default = "default_us10y_series_id")]
    pub series_id: String,
    #[serde(default = "default_us10y_symbol")]
    pub symbol: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_fred_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_tradfi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_tradfi_spread_bps")]
    pub spread_bps: f64,
}
fn default_yahoo_base_url() -> String {
    "https://query1.finance.yahoo.com/v8/finance/chart/".to_string()
}

fn default_dxy_yahoo_symbol() -> String {
    "DX-Y.NYB".to_string()
}

fn default_dxy_symbol() -> String {
    "DXY".to_string()
}

fn default_vix_yahoo_symbol() -> String {
    "^VIX".to_string()
}

fn default_vix_symbol() -> String {
    "VIX".to_string()
}

fn default_fred_base_url() -> String {
    "https://api.stlouisfed.org/fred/".to_string()
}

fn default_us10y_series_id() -> String {
    "DGS10".to_string()
}

fn default_us10y_symbol() -> String {
    "US10Y".to_string()
}

fn default_fred_api_key_env() -> String {
    "FRED_API_KEY".to_string()
}

fn default_tradfi_poll_secs() -> u64 {
    60
}

fn default_tradfi_spread_bps() -> f64 {
    1.0
}
impl Default for YahooIndicatorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_yahoo_base_url(),
            yahoo_symbol: default_dxy_yahoo_symbol(),
            symbol: default_dxy_symbol(),
            poll_secs: default_tradfi_poll_secs(),
            spread_bps: default_tradfi_spread_bps(),
        }
    }
}

impl Default for FredSeriesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_fred_base_url(),
            series_id: default_us10y_series_id(),
            symbol: default_us10y_symbol(),
            api_key: None,
            api_key_env: default_fred_api_key_env(),
            poll_secs: default_tradfi_poll_secs(),
            spread_bps: default_tradfi_spread_bps(),
        }
    }
}

pub fn default_vix_config() -> YahooIndicatorConfig {
    YahooIndicatorConfig {
        yahoo_symbol: default_vix_yahoo_symbol(),
        symbol: default_vix_symbol(),
        ..YahooIndicatorConfig::default()
    }
}

impl Default for TradfiConfig {
    fn default() -> Self {
        Self {
            dxy: YahooIndicatorConfig::default(),
            vix: default_vix_config(),
            us10y: FredSeriesConfig::default(),
        }
    }
}
