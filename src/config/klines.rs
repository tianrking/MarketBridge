use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct KlineConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_kline_sqlite_path")]
    pub sqlite_path: String,
    #[serde(default = "default_kline_intervals")]
    pub intervals: Vec<String>,
    #[serde(default = "default_kline_history_limit")]
    pub history_limit: usize,
    #[serde(default)]
    pub backfill_on_start: bool,
    #[serde(default = "default_kline_sources")]
    pub sources: Vec<String>,
}

impl Default for KlineConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sqlite_path: default_kline_sqlite_path(),
            intervals: default_kline_intervals(),
            history_limit: default_kline_history_limit(),
            backfill_on_start: false,
            sources: default_kline_sources(),
        }
    }
}

fn default_kline_sqlite_path() -> String {
    "data/marketbridge.sqlite".to_string()
}

fn default_kline_intervals() -> Vec<String> {
    vec![
        "1m".to_string(),
        "5m".to_string(),
        "15m".to_string(),
        "1h".to_string(),
    ]
}

fn default_kline_history_limit() -> usize {
    1500
}

fn default_kline_sources() -> Vec<String> {
    vec!["binance".to_string(), "okx".to_string()]
}
