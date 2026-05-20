use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct StrategyConfig {
    pub min_profit_usdt: f64,
    pub min_profit_bps: f64,
    pub min_signal_hold_ms: u64,
    pub slippage_bps: f64,
    #[serde(default)]
    pub fee_mode: StrategyFeeMode,
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
