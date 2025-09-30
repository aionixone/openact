//! Kinds API handlers

use crate::{
    dto::{KindSummary, ResponseEnvelope, ResponseMeta},
    middleware::{request_id::RequestId, tenant::Tenant},
    AppState,
};
use axum::{
    extract::{Extension, State},
    response::Json,
};
use openact_mcp::GovernanceConfig;
use serde_json::json;

/// GET /api/v1/kinds
pub async fn get_kinds(
    State((app_state, _governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
) -> Result<
    Json<ResponseEnvelope<serde_json::Value>>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    // Get metadata from registered connector factories (authoritative source)
    let connector_metadata = app_state.registry.connector_metadata();

    // Convert to KindSummary for API response
    let kinds: Vec<KindSummary> = connector_metadata
        .into_iter()
        .map(|metadata| KindSummary {
            name: metadata.kind.as_str().to_string(),
            description: metadata.description,
            category: metadata.category,
        })
        .collect();

    let response = ResponseEnvelope {
        success: true,
        data: json!({ "kinds": kinds }),
        metadata: ResponseMeta {
            request_id: request_id.0,
            tenant: Some(tenant.as_str().to_string()),
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}
