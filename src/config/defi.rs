use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DefiConfig {
    #[serde(default)]
    pub jupiter: JupiterConfig,
    #[serde(default)]
    pub meteora: DexScreenerConfig,
    #[serde(default)]
    pub orca: DexScreenerConfig,
    #[serde(default)]
    pub raydium: RaydiumConfig,
    #[serde(default)]
    pub uniswap_v3: UniswapV3Config,
    #[serde(default)]
    pub paraswap: ParaswapConfig,
    #[serde(default)]
    pub oneinch: OneInchConfig,
    #[serde(default)]
    pub pancakeswap: DexScreenerConfig,
    #[serde(default)]
    pub balancer: DexScreenerConfig,
    #[serde(default)]
    pub curve: DexScreenerConfig,
    #[serde(default)]
    pub sushiswap: DexScreenerConfig,
    #[serde(default)]
    pub quickswap: DexScreenerConfig,
    #[serde(default)]
    pub traderjoe: DexScreenerConfig,
    #[serde(default)]
    pub etcswap: DexScreenerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JupiterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_jupiter_base_url")]
    pub base_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_solana_quote_pairs")]
    pub pairs: Vec<SolanaQuotePair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaydiumConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_raydium_price_url")]
    pub price_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_raydium_pairs")]
    pub pairs: Vec<RaydiumPair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UniswapV3Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_uniswap_v3_subgraph_url")]
    pub subgraph_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_uniswap_v3_pools")]
    pub pools: Vec<UniswapV3Pool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParaswapConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_paraswap_base_url")]
    pub base_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_evm_quote_pairs")]
    pub pairs: Vec<EvmQuotePair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OneInchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_oneinch_base_url")]
    pub base_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_evm_quote_pairs")]
    pub pairs: Vec<EvmQuotePair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DexScreenerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_dexscreener_base_url")]
    pub base_url: String,
    #[serde(default = "default_defi_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_meteora_pairs")]
    pub pairs: Vec<DexScreenerPair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DexScreenerPair {
    pub symbol: String,
    pub chain_id: String,
    pub dex_id: String,
    pub query: String,
    #[serde(default = "default_defi_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolanaQuotePair {
    pub symbol: String,
    pub input_mint: String,
    pub output_mint: String,
    pub amount: u64,
    pub input_decimals: u8,
    pub output_decimals: u8,
    #[serde(default = "default_defi_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaydiumPair {
    pub symbol: String,
    pub base_mint: String,
    pub quote_mint: String,
    #[serde(default = "default_defi_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UniswapV3Pool {
    pub symbol: String,
    pub pool_id: String,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "default_defi_spread_bps")]
    pub spread_bps: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvmQuotePair {
    pub symbol: String,
    pub network: u64,
    pub src_token: String,
    pub dest_token: String,
    pub amount: String,
    pub src_decimals: u8,
    pub dest_decimals: u8,
    #[serde(default = "default_defi_spread_bps")]
    pub spread_bps: f64,
}
fn default_defi_poll_secs() -> u64 {
    10
}

fn default_defi_spread_bps() -> f64 {
    5.0
}

fn default_jupiter_base_url() -> String {
    "https://quote-api.jup.ag/v6/".to_string()
}

fn default_dexscreener_base_url() -> String {
    "https://api.dexscreener.com/".to_string()
}

fn default_raydium_price_url() -> String {
    "https://api.raydium.io/v2/main/price".to_string()
}

fn default_uniswap_v3_subgraph_url() -> String {
    "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3".to_string()
}

fn default_paraswap_base_url() -> String {
    "https://apiv5.paraswap.io/".to_string()
}

fn default_oneinch_base_url() -> String {
    "https://api.1inch.io/v5.0/".to_string()
}

fn default_solana_quote_pairs() -> Vec<SolanaQuotePair> {
    vec![SolanaQuotePair {
        symbol: "SOLUSDC".to_string(),
        input_mint: "So11111111111111111111111111111111111111112".to_string(),
        output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        amount: 1_000_000_000,
        input_decimals: 9,
        output_decimals: 6,
        spread_bps: default_defi_spread_bps(),
    }]
}

fn default_meteora_pairs() -> Vec<DexScreenerPair> {
    vec![DexScreenerPair {
        symbol: "SOLUSDC".to_string(),
        chain_id: "solana".to_string(),
        dex_id: "meteora".to_string(),
        query: "SOL USDC".to_string(),
        spread_bps: default_defi_spread_bps(),
    }]
}

fn default_raydium_pairs() -> Vec<RaydiumPair> {
    vec![RaydiumPair {
        symbol: "SOLUSDC".to_string(),
        base_mint: "So11111111111111111111111111111111111111112".to_string(),
        quote_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        spread_bps: default_defi_spread_bps(),
    }]
}

fn default_uniswap_v3_pools() -> Vec<UniswapV3Pool> {
    vec![UniswapV3Pool {
        symbol: "ETHUSDC".to_string(),
        pool_id: "0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8".to_string(),
        invert: false,
        spread_bps: default_defi_spread_bps(),
    }]
}

fn default_evm_quote_pairs() -> Vec<EvmQuotePair> {
    vec![EvmQuotePair {
        symbol: "ETHUSDC".to_string(),
        network: 1,
        src_token: "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string(),
        dest_token: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
        amount: "1000000000000000000".to_string(),
        src_decimals: 18,
        dest_decimals: 6,
        spread_bps: default_defi_spread_bps(),
    }]
}
impl Default for JupiterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_jupiter_base_url(),
            poll_secs: default_defi_poll_secs(),
            pairs: default_solana_quote_pairs(),
        }
    }
}

impl Default for DexScreenerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_dexscreener_base_url(),
            poll_secs: default_defi_poll_secs(),
            pairs: default_meteora_pairs(),
        }
    }
}

impl Default for RaydiumConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            price_url: default_raydium_price_url(),
            poll_secs: default_defi_poll_secs(),
            pairs: default_raydium_pairs(),
        }
    }
}

impl Default for UniswapV3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            subgraph_url: default_uniswap_v3_subgraph_url(),
            poll_secs: default_defi_poll_secs(),
            pools: default_uniswap_v3_pools(),
        }
    }
}

impl Default for ParaswapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_paraswap_base_url(),
            poll_secs: default_defi_poll_secs(),
            pairs: default_evm_quote_pairs(),
        }
    }
}

impl Default for OneInchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_oneinch_base_url(),
            poll_secs: default_defi_poll_secs(),
            pairs: default_evm_quote_pairs(),
        }
    }
}
