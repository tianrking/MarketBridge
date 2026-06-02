use serde::Deserialize;

use crate::types::BackpressureMode;

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub queue_capacity: usize,
    #[serde(default)]
    pub router_publish_queue_capacity: usize,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    #[serde(default = "default_event_bus_shards")]
    pub event_bus_shards: usize,
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
    #[serde(default)]
    pub clickhouse: ClickHouseConfig,
    #[serde(default = "default_order_flow_large_trade_notional_usdt")]
    pub order_flow_large_trade_notional_usdt: f64,
    #[serde(default = "default_ws_send_timeout_ms")]
    pub ws_send_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClickHouseConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_clickhouse_url")]
    pub url: String,
    #[serde(default = "default_clickhouse_database")]
    pub database: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub password_env: Option<String>,
    #[serde(default = "default_clickhouse_batch_max")]
    pub batch_max: usize,
    #[serde(default = "default_clickhouse_flush_ms")]
    pub flush_ms: u64,
    #[serde(default = "default_clickhouse_local_buffer")]
    pub local_buffer: usize,
    #[serde(default = "default_clickhouse_init_tables")]
    pub init_tables: bool,
}

impl Default for ClickHouseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: default_clickhouse_url(),
            database: default_clickhouse_database(),
            username: None,
            password: None,
            password_env: Some("CLICKHOUSE_PASSWORD".to_string()),
            batch_max: default_clickhouse_batch_max(),
            flush_ms: default_clickhouse_flush_ms(),
            local_buffer: default_clickhouse_local_buffer(),
            init_tables: default_clickhouse_init_tables(),
        }
    }
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

fn default_event_bus_shards() -> usize {
    1
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

fn default_clickhouse_url() -> String {
    "http://127.0.0.1:8123".to_string()
}

fn default_clickhouse_database() -> String {
    "marketbridge".to_string()
}

fn default_clickhouse_batch_max() -> usize {
    1_000
}

fn default_clickhouse_flush_ms() -> u64 {
    250
}

fn default_clickhouse_local_buffer() -> usize {
    100_000
}

fn default_clickhouse_init_tables() -> bool {
    true
}
