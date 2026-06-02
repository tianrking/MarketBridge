use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde_json::json;

use crate::config::RuntimeConfig;
use crate::types::now_ms;

#[derive(Debug, Clone)]
pub struct ApiAccessGuard {
    inner: Arc<ApiAccessGuardInner>,
}

#[derive(Debug)]
struct ApiAccessGuardInner {
    api_key: Option<String>,
    rate_limit_per_minute: u64,
    windows: DashMap<String, RateWindow>,
    last_prune_minute: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
struct RateWindow {
    minute: u64,
    count: u64,
}

impl ApiAccessGuard {
    pub fn from_runtime(cfg: &RuntimeConfig) -> Self {
        let api_key = cfg.api_key.clone().or_else(|| {
            cfg.api_key_env
                .as_ref()
                .and_then(|name| std::env::var(name).ok())
                .filter(|value| !value.trim().is_empty())
        });
        Self {
            inner: Arc::new(ApiAccessGuardInner {
                api_key,
                rate_limit_per_minute: cfg.api_rate_limit_per_minute,
                windows: DashMap::new(),
                last_prune_minute: AtomicU64::new(0),
            }),
        }
    }

    fn authorized(&self, req: &Request<Body>) -> bool {
        let Some(expected) = &self.inner.api_key else {
            return true;
        };
        req.headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == expected)
            || req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|value| value == expected)
    }

    fn allow_request(&self, req: &Request<Body>) -> bool {
        let limit = self.inner.rate_limit_per_minute;
        if limit == 0 {
            return true;
        }

        let client = client_key(req);
        let minute = now_ms() / 60_000;
        self.prune_rate_windows(minute);
        let mut allowed = true;
        self.inner
            .windows
            .entry(client)
            .and_modify(|window| {
                if window.minute != minute {
                    *window = RateWindow { minute, count: 1 };
                } else if window.count >= limit {
                    allowed = false;
                } else {
                    window.count += 1;
                }
            })
            .or_insert(RateWindow { minute, count: 1 });
        allowed
    }

    fn prune_rate_windows(&self, current_minute: u64) {
        let last = self.inner.last_prune_minute.load(Ordering::Relaxed);
        if current_minute <= last {
            return;
        }
        if self
            .inner
            .last_prune_minute
            .compare_exchange(last, current_minute, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let retain_after = current_minute.saturating_sub(5);
        self.inner
            .windows
            .retain(|_, window| window.minute >= retain_after);
    }
}

pub async fn api_guard(
    State(guard): State<ApiAccessGuard>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !guard.authorized(&req) {
        return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    if !guard.allow_request(&req) {
        return json_error(StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded");
    }
    next.run(req).await
}

fn client_key(req: &Request<Body>) -> String {
    req.headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(|value| format!("key:{value}"))
        .or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .map(|value| format!("ip:{}", value.trim()))
        })
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(|value| format!("ip:{value}"))
        })
        .unwrap_or_else(|| "anonymous".to_string())
}

fn json_error(status: StatusCode, error: &'static str) -> Response {
    (status, axum::Json(json!({ "error": error }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn authorization_accepts_bearer_or_api_key_header() {
        let guard = ApiAccessGuard {
            inner: Arc::new(ApiAccessGuardInner {
                api_key: Some("secret".to_string()),
                rate_limit_per_minute: 0,
                windows: DashMap::new(),
                last_prune_minute: AtomicU64::new(0),
            }),
        };
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .expect("test request builds");

        assert!(guard.authorized(&req));

        let mut req = Request::builder()
            .body(Body::empty())
            .expect("test request builds");
        req.headers_mut()
            .insert("x-api-key", HeaderValue::from_static("secret"));
        assert!(guard.authorized(&req));
    }

    #[test]
    fn rate_limit_blocks_after_limit() {
        let guard = ApiAccessGuard {
            inner: Arc::new(ApiAccessGuardInner {
                api_key: None,
                rate_limit_per_minute: 1,
                windows: DashMap::new(),
                last_prune_minute: AtomicU64::new(0),
            }),
        };
        let req = Request::builder()
            .header("x-forwarded-for", "127.0.0.1")
            .body(Body::empty())
            .expect("test request builds");

        assert!(guard.allow_request(&req));
        assert!(!guard.allow_request(&req));
    }

    #[test]
    fn rate_limit_prunes_stale_clients() {
        let guard = ApiAccessGuard {
            inner: Arc::new(ApiAccessGuardInner {
                api_key: None,
                rate_limit_per_minute: 10,
                windows: DashMap::new(),
                last_prune_minute: AtomicU64::new(0),
            }),
        };
        guard.inner.windows.insert(
            "ip:old".to_string(),
            RateWindow {
                minute: 1,
                count: 1,
            },
        );
        guard.inner.windows.insert(
            "ip:fresh".to_string(),
            RateWindow {
                minute: 10,
                count: 1,
            },
        );

        guard.prune_rate_windows(10);

        assert!(!guard.inner.windows.contains_key("ip:old"));
        assert!(guard.inner.windows.contains_key("ip:fresh"));
    }
}
