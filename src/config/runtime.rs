use serde::Deserialize;

use crate::types::BackpressureMode;

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub queue_capacity: usize,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    pub backpressure: BackpressureConfig,
    pub report_interval_ms: u64,
    pub stale_ttl_ms: u64,
    #[serde(default = "default_api_addr")]
    pub api_addr: String,
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default = "default_redis_stream_prefix")]
    pub redis_stream_prefix: String,
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

fn default_api_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_broadcast_capacity() -> usize {
    65_536
}

fn default_redis_stream_prefix() -> String {
    "ticks".to_string()
}
