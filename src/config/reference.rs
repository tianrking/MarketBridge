use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReferenceDataConfig {
    #[serde(default)]
    pub supply: SupplyConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupplyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_supply_provider")]
    pub provider: String,
    #[serde(default = "default_supply_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_supply_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_supply_poll_secs")]
    pub poll_secs: u64,
    #[serde(default)]
    pub assets: Vec<SupplyAssetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupplyAssetConfig {
    /// Stable local identity, not a ticker. Example: `lisk-v2`.
    pub asset_id: String,
    /// Provider-specific identity. For CoinGecko this is its coin ID.
    pub provider_asset_id: String,
    /// Explicit CEX perpetual symbols bound to this asset identity. Tickers are
    /// never inferred by the supply/OI join.
    pub perp_symbols: Vec<String>,
    /// Source explaining the identity mapping or migration.
    pub identity_evidence: String,
    #[serde(default)]
    pub chain: Option<String>,
    #[serde(default)]
    pub contract_address: Option<String>,
}

fn default_supply_provider() -> String {
    "coingecko".into()
}
fn default_supply_base_url() -> String {
    "https://api.coingecko.com/api/v3/".into()
}
fn default_supply_api_key_env() -> String {
    "COINGECKO_API_KEY".into()
}
fn default_supply_poll_secs() -> u64 {
    60
}

impl Default for SupplyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_supply_provider(),
            base_url: default_supply_base_url(),
            api_key: None,
            api_key_env: default_supply_api_key_env(),
            poll_secs: default_supply_poll_secs(),
            assets: Vec::new(),
        }
    }
}
