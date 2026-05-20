use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::types::BackpressureMode;

pub mod fees;
pub mod klines;
pub mod onchain;
pub mod runtime;
pub mod strategy;

pub use fees::ExchangeConfig;
pub use klines::KlineConfig;
pub use onchain::{EtherscanConfig, MempoolSpaceConfig, OnchainConfig, WhaleAlertConfig};
pub use runtime::RuntimeConfig;
pub use strategy::{StrategyConfig, StrategyFeeMode};

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

#[cfg(test)]
mod tests {
    use super::fees::{FeeModel, FeeTier};

    #[test]
    fn tiered_fee_selects_highest_matching_tier() {
        let f = FeeModel::Tiered {
            volume_30d_usdt: 5_500_000.0,
            tiers: vec![
                FeeTier {
                    min_volume_usdt: 0.0,
                    maker_bps: 10.0,
                    taker_bps: 12.0,
                },
                FeeTier {
                    min_volume_usdt: 1_000_000.0,
                    maker_bps: 8.0,
                    taker_bps: 9.0,
                },
                FeeTier {
                    min_volume_usdt: 5_000_000.0,
                    maker_bps: 6.0,
                    taker_bps: 7.0,
                },
            ],
        };
        assert!((f.maker_bps() - 6.0).abs() < 1e-9);
        assert!((f.taker_bps() - 7.0).abs() < 1e-9);
    }
}
