use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;

use crate::api::ApiState;
use crate::data_lake::{LakeDeleteQuery, LakeManifestQuery};

pub async fn manifest(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<LakeManifestQuery>,
) -> impl IntoResponse {
    match state.data_lake_store.manifest(q).await {
        Ok(partitions) => Json(
            serde_json::json!({"version":"v1","domain":"storage_manifest","partitions":partitions}),
        ),
        Err(error) => Json(serde_json::json!({
            "version":"v1",
            "domain":"storage_manifest",
            "error": error.to_string(),
            "partitions": []
        })),
    }
}

pub async fn delete_partitions(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<LakeDeleteQuery>,
) -> impl IntoResponse {
    match state.data_lake_store.delete_partitions(q).await {
        Ok(result) => {
            Json(serde_json::json!({"version":"v1","domain":"storage_delete","result":result}))
        }
        Err(error) => Json(serde_json::json!({
            "version":"v1",
            "domain":"storage_delete",
            "error": error.to_string()
        })),
    }
}
