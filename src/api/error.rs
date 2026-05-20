use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub fn upstream_error(source: &'static str, error: impl ToString) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "source": source,
            "error": error.to_string()
        })),
    )
        .into_response()
}

pub fn multi_status(body: serde_json::Value) -> Response {
    (StatusCode::MULTI_STATUS, Json(body)).into_response()
}
