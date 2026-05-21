use serde::Deserialize;

use crate::types::BackpressureMode;

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub queue_capacity: usize,
    #[serde(default)]
    pub router_publish_queue_capacity: usize,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    pub backpressure: BackpressureConfig,
    pub report_interval_ms: u64,
    pub stale_ttl_ms: u64,
    #[serde(default = "default_api_addr")]
    pub api_addr: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_rate_limit_per_minute: u64,
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default = "default_redis_stream_prefix")]
    pub redis_stream_prefix: String,
    #[serde(default = "default_redis_dead_letter_path")]
    pub redis_dead_letter_path: String,
    #[serde(default = "default_order_flow_large_trade_notional_usdt")]
    pub order_flow_large_trade_notional_usdt: f64,
    #[serde(default = "default_ws_send_timeout_ms")]
    pub ws_send_timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressureConfig {
    Block,
    DropNewest,
}

impl BackpressureConfig {
    pub fn mode(self) -> BackpressureMode {
        match self {
            BackpressureConfig::Block => BackpressureMode::Block,
            BackpressureConfig::DropNewest => BackpressureMode::DropNewest,
        }
    }
}

impl RuntimeConfig {
    pub fn router_publish_queue_capacity(&self) -> usize {
        if self.router_publish_queue_capacity == 0 {
            self.queue_capacity
        } else {
            self.router_publish_queue_capacity
        }
    }
}

fn default_api_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_api_key_env() -> Option<String> {
    Some("MARKETBRIDGE_API_KEY".to_string())
}

fn default_broadcast_capacity() -> usize {
    65_536
}

fn default_redis_stream_prefix() -> String {
    "ticks".to_string()
}

fn default_redis_dead_letter_path() -> String {
    "data/redis_dead_letters.jsonl".to_string()
}

fn default_order_flow_large_trade_notional_usdt() -> f64 {
    100_000.0
}

fn default_ws_send_timeout_ms() -> u64 {
    3_000
}
