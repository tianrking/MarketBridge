use crate::config::{CustomApiConfig, ProviderQuotaConfig};
use crate::connectors::aggregate::common::parse_f64_value;
use crate::source::{ExchangeSource, SourceContext};
use crate::types::{DataEvent, ExternalSignalTick, now_ms};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime},
};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::warn;

type PacingBudgets = Arc<Mutex<HashMap<String, Option<Instant>>>>;
type QuotaBudgets = Arc<Mutex<HashMap<String, QuotaWindow>>>;
static PACING_BUDGETS: OnceLock<PacingBudgets> = OnceLock::new();
static QUOTA_BUDGETS: OnceLock<QuotaBudgets> = OnceLock::new();
static QUOTA_POLICIES: OnceLock<Arc<HashMap<String, QuotaPolicy>>> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
struct QuotaPolicy {
    max_requests: u32,
    window: Duration,
}

#[derive(Clone, Copy, Debug)]
struct QuotaWindow {
    started_at: Instant,
    used_requests: u32,
}

#[derive(Debug, Serialize)]
pub struct ProviderQuotaStatus {
    pub group: String,
    pub max_requests: u32,
    pub window_secs: u64,
    pub used_requests: u32,
    pub remaining_requests: u32,
    pub window_remaining_ms: u64,
}

pub struct CustomApiPoller {
    cfg: CustomApiConfig,
    client: reqwest::Client,
    provider_quotas: Arc<HashMap<String, ProviderQuotaConfig>>,
}
impl CustomApiPoller {
    pub fn new(
        cfg: CustomApiConfig,
        provider_quotas: Arc<HashMap<String, ProviderQuotaConfig>>,
    ) -> Self {
        QUOTA_POLICIES.get_or_init(|| {
            Arc::new(
                provider_quotas
                    .iter()
                    .map(|(name, quota)| {
                        (
                            name.clone(),
                            QuotaPolicy {
                                max_requests: quota.max_requests,
                                window: Duration::from_secs(quota.window_secs),
                            },
                        )
                    })
                    .collect(),
            )
        });
        Self {
            cfg,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("static HTTP client settings"),
            provider_quotas,
        }
    }
}

pub async fn provider_quota_status() -> Vec<ProviderQuotaStatus> {
    let Some(policies) = QUOTA_POLICIES.get() else {
        return Vec::new();
    };
    let now = Instant::now();
    let windows = match QUOTA_BUDGETS.get() {
        Some(windows) => windows.lock().await,
        None => {
            return policies
                .iter()
                .map(|(group, policy)| ProviderQuotaStatus {
                    group: group.clone(),
                    max_requests: policy.max_requests,
                    window_secs: policy.window.as_secs(),
                    used_requests: 0,
                    remaining_requests: policy.max_requests,
                    window_remaining_ms: 0,
                })
                .collect();
        }
    };
    let mut rows = policies
        .iter()
        .map(|(group, policy)| {
            let (used, remaining_ms) = match windows.get(group) {
                Some(window)
                    if now.saturating_duration_since(window.started_at) < policy.window =>
                {
                    let remaining_ms = policy
                        .window
                        .saturating_sub(now.saturating_duration_since(window.started_at))
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64;
                    (window.used_requests, remaining_ms)
                }
                _ => (0, 0),
            };
            ProviderQuotaStatus {
                group: group.clone(),
                max_requests: policy.max_requests,
                window_secs: policy.window.as_secs(),
                used_requests: used,
                remaining_requests: policy.max_requests.saturating_sub(used),
                window_remaining_ms: remaining_ms,
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.group.cmp(&b.group));
    rows
}

async fn reserve_pacing(budgets: &PacingBudgets, scope: &str, interval: Duration) {
    loop {
        let wait = {
            let mut map = budgets.lock().await;
            let now = Instant::now();
            let next = map.entry(scope.into()).or_insert(Some(now));
            if next.is_some_and(|next| next <= now) {
                *next = now.checked_add(interval);
                return;
            }
            *next
        };
        match wait {
            Some(wait) => tokio::time::sleep_until(wait).await,
            None => std::future::pending::<()>().await,
        }
    }
}

fn quota_policy(
    quotas: &HashMap<String, ProviderQuotaConfig>,
    group: Option<&str>,
) -> Option<QuotaPolicy> {
    let quota = quotas.get(group?)?;
    Some(QuotaPolicy {
        max_requests: quota.max_requests,
        window: Duration::from_secs(quota.window_secs),
    })
}

fn reserve_quota_window(
    windows: &mut HashMap<String, QuotaWindow>,
    group: &str,
    weight: u32,
    policy: QuotaPolicy,
    now: Instant,
) -> Option<Instant> {
    let window = windows.entry(group.to_string()).or_insert(QuotaWindow {
        started_at: now,
        used_requests: 0,
    });
    if now.saturating_duration_since(window.started_at) >= policy.window {
        *window = QuotaWindow {
            started_at: now,
            used_requests: 0,
        };
    }
    if window.used_requests.saturating_add(weight) <= policy.max_requests {
        window.used_requests += weight;
        None
    } else {
        window.started_at.checked_add(policy.window)
    }
}

async fn reserve_quota(group: &str, weight: u32, policy: QuotaPolicy) {
    let budgets = QUOTA_BUDGETS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
    loop {
        let wait = {
            let mut windows = budgets.lock().await;
            reserve_quota_window(&mut windows, group, weight, policy, Instant::now())
        };
        match wait {
            Some(wait) => tokio::time::sleep_until(wait).await,
            None => return,
        }
    }
}

#[async_trait]
impl ExchangeSource for CustomApiPoller {
    fn name(&self) -> &'static str {
        "custom_api"
    }
    fn label(&self) -> String {
        format!("custom_api/{}", self.cfg.name)
    }
    async fn run(&self, ctx: SourceContext) -> Result<()> {
        // Origin pacing is supplemented by an optional shared weighted provider quota.
        let scope = url::Url::parse(&self.cfg.url)?
            .origin()
            .ascii_serialization();
        let pacing_budgets = PACING_BUDGETS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
        let interval = Duration::from_secs(self.cfg.poll_secs.max(1));
        let quota = quota_policy(&self.provider_quotas, self.cfg.quota_group.as_deref());
        loop {
            reserve_pacing(pacing_budgets, &scope, interval).await;
            if let (Some(group), Some(policy)) = (self.cfg.quota_group.as_deref(), quota) {
                reserve_quota(group, self.cfg.quota_weight, policy).await;
            }
            match fetch_custom_value(&self.client, &self.cfg).await {
                Ok(FetchOutcome::Value(value, source_time_ms, raw)) => {
                    ctx.emit(DataEvent::ExternalSignal(ExternalSignalTick {
                        source: "custom_api",
                        source_instance: Some(self.cfg.name.clone().into_boxed_str()),
                        source_time_ms,
                        category: self.cfg.category.clone().into_boxed_str(),
                        symbol: self
                            .cfg
                            .symbol
                            .as_ref()
                            .map(|s| s.to_ascii_uppercase().into_boxed_str()),
                        metric: self.cfg.metric.clone().into_boxed_str(),
                        value: Some(value),
                        score: None,
                        title: None,
                        url: None,
                        ts_ms: now_ms(),
                        raw: Some(raw),
                    }))
                    .await?;
                }
                Ok(FetchOutcome::Backoff(delay)) => {
                    let mut map = pacing_budgets.lock().await;
                    let next = Instant::now().checked_add(delay.max(interval));
                    // None pauses only this origin until restart; no mutex is
                    // held while waiting and other providers remain independent.
                    map.entry(scope.clone())
                        .and_modify(|old| {
                            *old = old.zip(next).map(|(a, b)| a.max(b));
                        })
                        .or_insert(next);
                    warn!(source=%self.cfg.name,delay_secs=delay.as_secs(),"custom API quota backoff");
                }
                Err(error) => warn!(source=%self.cfg.name,%error,"custom API refresh failed"),
            }
            tokio::time::sleep(interval).await;
        }
    }
}

enum FetchOutcome {
    Value(f64, Option<u64>, Value),
    Backoff(Duration),
}

fn retry_delay(header: Option<&str>, now: SystemTime) -> Duration {
    header
        .and_then(|s| {
            s.parse::<u64>().ok().map(Duration::from_secs).or_else(|| {
                httpdate::parse_http_date(s)
                    .ok()
                    .map(|t| t.duration_since(now).unwrap_or_default())
            })
        })
        .unwrap_or(Duration::from_secs(60))
        .max(Duration::from_secs(1))
}

async fn fetch_custom_value(
    client: &reqwest::Client,
    cfg: &CustomApiConfig,
) -> Result<FetchOutcome> {
    let mut response = client.get(&cfg.url).send().await?;
    if matches!(response.status().as_u16(), 418 | 429 | 503) {
        return Ok(FetchOutcome::Backoff(retry_delay(
            response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            SystemTime::now(),
        )));
    }
    ensure!(
        !response.status().is_redirection(),
        "custom API redirects require explicit configuration"
    );
    response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 1024 * 1024,
            "custom API body exceeds 1 MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes).context("custom API requires UTF-8")?;
    let (value, ts, raw) = parse_observation(text, cfg)?;
    Ok(FetchOutcome::Value(value, ts, raw))
}

fn parse_observation(text: &str, cfg: &CustomApiConfig) -> Result<(f64, Option<u64>, Value)> {
    let raw: Value = serde_json::from_str(text).context("custom API response must be JSON")?;
    let value = value_at_path(&raw, &cfg.value_path)
        .and_then(parse_f64_value)
        .filter(|v| v.is_finite())
        .context("mapped value is missing, null or non-finite")?;
    let source_time = cfg
        .timestamp_path
        .as_ref()
        .map(|path| -> Result<u64> {
            let field = value_at_path(&raw, path).context("mapped timestamp missing")?;
            let value = field
                .as_u64()
                .or_else(|| field.as_str().and_then(|s| s.parse().ok()))
                .context("timestamp must be a positive integer")?;
            ensure!(value > 0, "timestamp must be positive");
            if cfg.timestamp_in_seconds {
                value.checked_mul(1000).context("timestamp overflow")
            } else {
                Ok(value)
            }
        })
        .transpose()?;
    Ok((value, source_time, raw))
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return Some(value);
    }
    let mut current = value;
    for part in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_api_value_path_reads_nested_json() {
        let raw = serde_json::json!({"data":[{"price":"123.45"}]});
        assert_eq!(
            value_at_path(&raw, "data.0.price").and_then(parse_f64_value),
            Some(123.45)
        );
    }
    #[test]
    fn respects_seconds_and_http_date_retry_after() {
        let now = SystemTime::UNIX_EPOCH;
        assert_eq!(retry_delay(Some("120"), now), Duration::from_secs(120));
        assert_eq!(
            retry_delay(Some("Thu, 01 Jan 1970 00:02:00 GMT"), now),
            Duration::from_secs(120)
        );
        assert_eq!(retry_delay(None, now), Duration::from_secs(60));
    }
    #[test]
    fn mapped_time_is_explicit_and_missing_values_fail() {
        let cfg: CustomApiConfig = serde_json::from_value(serde_json::json!({
            "name":"gold","url":"https://example.invalid","metric":"price","value_path":"p",
            "timestamp_path":"t","timestamp_in_seconds":true}))
        .unwrap();
        let (price, ts, _) = parse_observation(r#"{"p":123.4,"t":100}"#, &cfg).unwrap();
        assert_eq!(price, 123.4);
        assert_eq!(ts, Some(100_000));
        assert!(parse_observation(r#"{"p":null,"t":100}"#, &cfg).is_err());
        assert!(parse_observation(r#"{"p":123.4}"#, &cfg).is_err());
    }

    #[test]
    fn weighted_quota_waits_until_its_window_then_resets() {
        let policy = QuotaPolicy {
            max_requests: 5,
            window: Duration::from_secs(60),
        };
        let now = Instant::now();
        let mut windows = HashMap::new();
        assert_eq!(
            reserve_quota_window(&mut windows, "public", 3, policy, now),
            None
        );
        let wait = reserve_quota_window(&mut windows, "public", 3, policy, now)
            .expect("second request exceeds the shared weighted quota");
        assert_eq!(wait.duration_since(now), Duration::from_secs(60));
        assert_eq!(
            reserve_quota_window(&mut windows, "public", 5, policy, wait),
            None
        );
    }

    #[test]
    fn quota_policy_requires_a_declared_group() {
        let quotas = HashMap::from([(
            "public".to_string(),
            ProviderQuotaConfig {
                name: "public".to_string(),
                max_requests: 30,
                window_secs: 60,
            },
        )]);
        assert!(quota_policy(&quotas, Some("public")).is_some());
        assert!(quota_policy(&quotas, Some("unknown")).is_none());
        assert!(quota_policy(&quotas, None).is_none());
    }
}
