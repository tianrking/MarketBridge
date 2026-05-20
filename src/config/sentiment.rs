use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SentimentConfig {
    #[serde(default)]
    pub fear_greed: FearGreedConfig,
    #[serde(default)]
    pub cryptopanic: CryptoPanicConfig,
    #[serde(default)]
    pub santiment: SantimentConfig,
    #[serde(default)]
    pub lunarcrush: LunarCrushConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FearGreedConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_fear_greed_url")]
    pub url: String,
    #[serde(default = "default_sentiment_poll_secs")]
    pub poll_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CryptoPanicConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cryptopanic_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_cryptopanic_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_sentiment_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_cryptopanic_currencies")]
    pub currencies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SantimentConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_santiment_url")]
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_santiment_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_sentiment_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_santiment_metrics")]
    pub metrics: Vec<SantimentMetric>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SantimentMetric {
    pub slug: String,
    pub metric: String,
    #[serde(default = "default_santiment_interval")]
    pub interval: String,
    #[serde(default = "default_santiment_from")]
    pub from: String,
    #[serde(default = "default_santiment_to")]
    pub to: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LunarCrushConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_lunarcrush_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_lunarcrush_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_sentiment_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_lunarcrush_symbols")]
    pub symbols: Vec<String>,
}
fn default_fear_greed_url() -> String {
    "https://api.alternative.me/fng/".to_string()
}

fn default_cryptopanic_base_url() -> String {
    "https://cryptopanic.com/api/v1/".to_string()
}

fn default_cryptopanic_api_key_env() -> String {
    "CRYPTOPANIC_API_KEY".to_string()
}

fn default_cryptopanic_currencies() -> Vec<String> {
    vec!["BTC".to_string(), "ETH".to_string()]
}

fn default_santiment_url() -> String {
    "https://api.santiment.net/graphql".to_string()
}

fn default_santiment_api_key_env() -> String {
    "SANTIMENT_API_KEY".to_string()
}

fn default_santiment_interval() -> String {
    "1h".to_string()
}

fn default_santiment_from() -> String {
    "utc_now-2h".to_string()
}

fn default_santiment_to() -> String {
    "utc_now".to_string()
}

fn default_santiment_metrics() -> Vec<SantimentMetric> {
    vec![SantimentMetric {
        slug: "bitcoin".to_string(),
        metric: "social_volume_total".to_string(),
        interval: default_santiment_interval(),
        from: default_santiment_from(),
        to: default_santiment_to(),
    }]
}

fn default_lunarcrush_base_url() -> String {
    "https://lunarcrush.com/api4/public/".to_string()
}

fn default_lunarcrush_api_key_env() -> String {
    "LUNARCRUSH_API_KEY".to_string()
}

fn default_lunarcrush_symbols() -> Vec<String> {
    vec!["BTC".to_string(), "ETH".to_string()]
}

fn default_sentiment_poll_secs() -> u64 {
    300
}
impl Default for FearGreedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_fear_greed_url(),
            poll_secs: default_sentiment_poll_secs(),
        }
    }
}

impl Default for CryptoPanicConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_cryptopanic_base_url(),
            api_key: None,
            api_key_env: default_cryptopanic_api_key_env(),
            poll_secs: default_sentiment_poll_secs(),
            currencies: default_cryptopanic_currencies(),
        }
    }
}

impl Default for SantimentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_santiment_url(),
            api_key: None,
            api_key_env: default_santiment_api_key_env(),
            poll_secs: default_sentiment_poll_secs(),
            metrics: default_santiment_metrics(),
        }
    }
}

impl Default for LunarCrushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_lunarcrush_base_url(),
            api_key: None,
            api_key_env: default_lunarcrush_api_key_env(),
            poll_secs: default_sentiment_poll_secs(),
            symbols: default_lunarcrush_symbols(),
        }
    }
}
