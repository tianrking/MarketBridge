use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderName, HeaderValue, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::{CorsConfig, RuntimeConfig};

const ALLOW_METHODS: &str = "GET,POST,DELETE,OPTIONS";
const ALLOW_HEADERS: &str = "content-type,x-api-key,authorization,accept";
const VARY_HEADERS: &str = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers, Access-Control-Request-Private-Network";

fn allow_private_network_header() -> HeaderName {
    HeaderName::from_static("access-control-allow-private-network")
}

fn request_private_network_header() -> HeaderName {
    HeaderName::from_static("access-control-request-private-network")
}

#[derive(Debug, Clone)]
pub struct ApiCors {
    enabled: bool,
    allowed_origins: Vec<String>,
    allow_private_network: bool,
    max_age_secs: u64,
}

impl ApiCors {
    pub fn from_runtime(cfg: &RuntimeConfig) -> Self {
        Self::from_config(&cfg.cors)
    }

    fn from_config(cfg: &CorsConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            allowed_origins: cfg
                .allowed_origins
                .iter()
                .map(|origin| origin.trim().to_ascii_lowercase())
                .filter(|origin| !origin.is_empty())
                .collect(),
            allow_private_network: cfg.allow_private_network,
            max_age_secs: cfg.max_age_secs,
        }
    }

    fn allowed_origin<'a>(&self, origin: &'a str) -> Option<&'a str> {
        if !self.enabled {
            return None;
        }
        let origin_normalized = origin.trim().to_ascii_lowercase();
        self.allowed_origins
            .iter()
            .any(|pattern| origin_matches_pattern(&origin_normalized, pattern))
            .then_some(origin)
    }

    fn private_network_requested(req: &Request<Body>) -> bool {
        req.headers()
            .get(request_private_network_header())
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }
}

pub async fn preflight() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

pub async fn cors_middleware(
    State(cors): State<ApiCors>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let allowed_origin = origin
        .as_deref()
        .and_then(|origin| cors.allowed_origin(origin))
        .map(str::to_string);
    let private_network_requested = ApiCors::private_network_requested(&req);
    let is_preflight = req.method() == Method::OPTIONS;

    if is_preflight {
        if origin.is_some() && allowed_origin.is_none() {
            return StatusCode::FORBIDDEN.into_response();
        }
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_cors_headers(
            response.headers_mut(),
            allowed_origin.as_deref(),
            cors.allow_private_network && private_network_requested,
            cors.max_age_secs,
        );
        return response;
    }

    let mut response = next.run(req).await;
    apply_cors_headers(
        response.headers_mut(),
        allowed_origin.as_deref(),
        false,
        cors.max_age_secs,
    );
    response
}

fn apply_cors_headers(
    headers: &mut axum::http::HeaderMap,
    allowed_origin: Option<&str>,
    allow_private_network: bool,
    max_age_secs: u64,
) {
    if let Some(origin) = allowed_origin.and_then(|origin| HeaderValue::from_str(origin).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(ALLOW_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(ALLOW_HEADERS),
    );
    headers.insert(header::VARY, HeaderValue::from_static(VARY_HEADERS));
    if let Ok(max_age) = HeaderValue::from_str(&max_age_secs.to_string()) {
        headers.insert(header::ACCESS_CONTROL_MAX_AGE, max_age);
    }
    if allow_private_network {
        headers.insert(
            allow_private_network_header(),
            HeaderValue::from_static("true"),
        );
    }
}

fn origin_matches_pattern(origin: &str, pattern: &str) -> bool {
    if pattern == "*" || origin == pattern {
        return true;
    }
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return false;
    };
    origin.starts_with(prefix) && origin.ends_with(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cors_with_origins(origins: &[&str]) -> ApiCors {
        ApiCors::from_config(&CorsConfig {
            enabled: true,
            allowed_origins: origins.iter().map(|origin| origin.to_string()).collect(),
            allow_private_network: true,
            max_age_secs: 600,
        })
    }

    #[test]
    fn wildcard_origin_patterns_match_pages_and_localhost() {
        let cors = cors_with_origins(&[
            "http://localhost:*",
            "http://127.0.0.1:*",
            "https://*.pages.dev",
        ]);

        assert_eq!(
            cors.allowed_origin("https://marketbridge-ui.pages.dev"),
            Some("https://marketbridge-ui.pages.dev")
        );
        assert_eq!(
            cors.allowed_origin("http://127.0.0.1:5173"),
            Some("http://127.0.0.1:5173")
        );
        assert_eq!(cors.allowed_origin("https://example.com"), None);
    }

    #[test]
    fn disabled_cors_rejects_browser_origins() {
        let cors = ApiCors::from_config(&CorsConfig {
            enabled: false,
            allowed_origins: vec!["*".to_string()],
            allow_private_network: true,
            max_age_secs: 600,
        });

        assert_eq!(
            cors.allowed_origin("https://marketbridge-ui.pages.dev"),
            None
        );
    }
}
