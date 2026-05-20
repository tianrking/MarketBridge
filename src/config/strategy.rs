use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub min_profit_usdt: f64,
    pub min_profit_bps: f64,
    pub min_signal_hold_ms: u64,
    pub slippage_bps: f64,
    #[serde(default)]
    pub fee_mode: StrategyFeeMode,
    #[serde(default = "default_book_signal_notional_usdt")]
    pub book_signal_notional_usdt: f64,
    #[serde(default = "default_fallback_maker_fee_bps")]
    pub fallback_maker_fee_bps: f64,
    #[serde(default = "default_fallback_taker_fee_bps")]
    pub fallback_taker_fee_bps: f64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyFeeMode {
    #[default]
    Taker,
    Maker,
    MakerBuyTakerSell,
    TakerBuyMakerSell,
}

fn default_book_signal_notional_usdt() -> f64 {
    1_000.0
}

fn default_fallback_maker_fee_bps() -> f64 {
    2.0
}

fn default_fallback_taker_fee_bps() -> f64 {
    10.0
}
