use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OnchainConfig {
    #[serde(default)]
    pub whale_alert: WhaleAlertConfig,
    #[serde(default)]
    pub mempool_space: MempoolSpaceConfig,
    #[serde(default)]
    pub etherscan: EtherscanConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WhaleAlertConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_whale_alert_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_whale_alert_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_onchain_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_whale_min_value_usd")]
    pub min_value_usd: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MempoolSpaceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mempool_space_base_url")]
    pub base_url: String,
    #[serde(default = "default_onchain_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_btc_large_transfer_btc")]
    pub min_value_btc: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EtherscanConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_etherscan_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_etherscan_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_onchain_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_eth_large_transfer_eth")]
    pub min_value_eth: f64,
    #[serde(default)]
    pub addresses: Vec<String>,
}

impl Default for WhaleAlertConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_whale_alert_base_url(),
            api_key: None,
            api_key_env: default_whale_alert_api_key_env(),
            poll_secs: default_onchain_poll_secs(),
            min_value_usd: default_whale_min_value_usd(),
        }
    }
}

impl Default for MempoolSpaceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_mempool_space_base_url(),
            poll_secs: default_onchain_poll_secs(),
            min_value_btc: default_btc_large_transfer_btc(),
        }
    }
}

impl Default for EtherscanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_etherscan_base_url(),
            api_key: None,
            api_key_env: default_etherscan_api_key_env(),
            poll_secs: default_onchain_poll_secs(),
            min_value_eth: default_eth_large_transfer_eth(),
            addresses: Vec::new(),
        }
    }
}

fn default_onchain_poll_secs() -> u64 {
    60
}

fn default_whale_alert_base_url() -> String {
    "https://api.whale-alert.io/v1/".to_string()
}

fn default_whale_alert_api_key_env() -> String {
    "WHALE_ALERT_API_KEY".to_string()
}

fn default_whale_min_value_usd() -> f64 {
    500_000.0
}

fn default_mempool_space_base_url() -> String {
    "https://mempool.space/api/".to_string()
}

fn default_btc_large_transfer_btc() -> f64 {
    100.0
}

fn default_etherscan_base_url() -> String {
    "https://api.etherscan.io/api".to_string()
}

fn default_etherscan_api_key_env() -> String {
    "ETHERSCAN_API_KEY".to_string()
}

fn default_eth_large_transfer_eth() -> f64 {
    1_000.0
}
