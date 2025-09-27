//! Health check handlers

use crate::{
    dto::{ResponseEnvelope, ResponseMeta},
    middleware::request_id::RequestId,
    AppState,
};
use axum::{
    extract::{Extension, State},
    response::Json,
};
use openact_mcp::GovernanceConfig;
use serde_json::json;

/// GET /api/v1/health
pub async fn health_check(
    State((_app_state, _governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
) -> Json<ResponseEnvelope<serde_json::Value>> {
    let response = ResponseEnvelope {
        success: true,
        data: json!({
            "status": "healthy",
            "service": "openact-server",
            "version": env!("CARGO_PKG_VERSION")
        }),
        metadata: ResponseMeta {
            request_id: request_id.0,
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Json(response)
}
