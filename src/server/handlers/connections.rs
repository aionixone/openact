#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::ConnectionUpsertRequest;
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{
    Json,
    extract::{Path, Query},
    response::IntoResponse,
};
use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListQuery {
    pub auth_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/connections",
    tag = "connections",
    operation_id = "connections_list",
    summary = "List connections",
    description = "Retrieve a list of connections with optional filtering",
    params(
        ("auth_type" = Option<String>, Query, description = "Filter by authorization type (api_key, basic, oauth2_client_credentials, oauth2_authorization_code)"),
        ("limit" = Option<i64>, Query, description = "Maximum number of connections to return"),
        ("offset" = Option<i64>, Query, description = "Number of connections to skip for pagination")
    ),
    responses(
        (status = 200, description = "List of connections", body = Vec<crate::models::ConnectionConfig>),
        (status = 400, description = "Invalid query parameters", body = crate::interface::error::ApiError),
        (status = 401, description = "Unauthorized - Missing or invalid authentication", body = crate::interface::error::ApiError),
        (status = 403, description = "Forbidden - Insufficient permissions", body = crate::interface::error::ApiError),
        (status = 429, description = "Too Many Requests - Rate limit exceeded", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn list(Query(q): Query<ListQuery>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc
        .list_connections(q.auth_type.as_deref(), q.limit, q.offset)
        .await
    {
        Ok(list) => Json(serde_json::json!(list)).into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connections",
    tag = "connections",
    operation_id = "connections_create",
    summary = "Create connection",
    description = "Create a new connection configuration",
    request_body = ConnectionUpsertRequest,
    responses(
        (status = 201, description = "Connection created successfully", body = crate::models::ConnectionConfig),
        (status = 400, description = "Invalid connection data", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn create(Json(req): Json<ConnectionUpsertRequest>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    
    // Validate TRN format
    use crate::utils::trn::parse_connection_trn;
    if let Err(e) = parse_connection_trn(&req.trn) {
        return helpers::validation_error("invalid_input", format!("Invalid TRN format: {}", e)).into_response();
    }
    
    // Convert DTO to config with metadata (new creation)
    let config = req.to_config(None, None);
    
    match svc.upsert_connection(&config).await {
        Ok(_) => (axum::http::StatusCode::CREATED, Json(serde_json::json!(config))).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/connections/{trn}",
    tag = "connections",
    operation_id = "connections_get",
    summary = "Get connection by TRN",
    description = "Retrieve a specific connection configuration by its TRN",
    params(
        ("trn" = String, Path, description = "Connection TRN identifier")
    ),
    responses(
        (status = 200, description = "Connection found", body = crate::models::ConnectionConfig),
        (status = 400, description = "Invalid TRN format", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn get(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.get_connection(&trn).await {
        Ok(Some(conn)) => Json(serde_json::json!(conn)).into_response(),
        Ok(None) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/v1/connections/{trn}",
    tag = "connections",
    operation_id = "connections_update",
    summary = "Update connection",
    description = "Update an existing connection configuration",
    params(
        ("trn" = String, Path, description = "Connection TRN identifier")
    ),
    request_body = ConnectionUpsertRequest,
    responses(
        (status = 200, description = "Connection updated successfully", body = crate::models::ConnectionConfig),
        (status = 400, description = "Invalid TRN format or data", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn update(
    Path(trn): Path<String>,
    Json(req): Json<ConnectionUpsertRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    if req.trn != trn {
        return helpers::validation_error("trn_mismatch", "trn mismatch").into_response();
    }
    
    let svc = OpenActService::from_env().await.unwrap();
    
    // Get existing version and created_at for proper versioning
    let (existing_version, existing_created_at) = match svc.get_connection(&trn).await {
        Ok(Some(existing)) => (Some(existing.version), Some(existing.created_at)),
        Ok(None) => (None, None), // Treat as creation if doesn't exist
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };
    
    // Convert DTO to config with proper versioning
    let config = req.to_config(existing_version, existing_created_at);
    
    match svc.upsert_connection(&config).await {
        Ok(_) => Json(serde_json::json!(config)).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/connections/{trn}",
    tag = "connections",
    operation_id = "connections_delete",
    summary = "Delete connection",
    description = "Delete a connection configuration",
    params(
        ("trn" = String, Path, description = "Connection TRN identifier")
    ),
    responses(
        (status = 204, description = "Connection deleted successfully"),
        (status = 400, description = "Invalid TRN format", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn del(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.delete_connection(&trn).await {
        Ok(true) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Ok(false) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/connections/{trn}/status",
    tag = "connections",
    operation_id = "connections_get_status",
    summary = "Get connection status",
    description = "Get authentication status for a connection (no network call)",
    params(
        ("trn" = String, Path, description = "Connection TRN identifier")
    ),
    responses(
        (status = 200, description = "Connection status", body = crate::interface::dto::ConnectionStatusDto),
        (status = 400, description = "Invalid TRN format", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
/// Get connection auth status (no network)
pub async fn status(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.connection_status(&trn).await {
        Ok(Some(s)) => Json(serde_json::json!(s)).into_response(),
        Ok(None) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConnectionTestRequest {
    #[serde(default = "default_test_endpoint")]
    pub endpoint: String,
}

fn default_test_endpoint() -> String { "https://httpbin.org/get".to_string() }

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connections/{trn}/test",
    tag = "connections",
    operation_id = "connections_test",
    summary = "Test connection",
    description = "Test a connection by performing a network request to verify authentication",
    params(
        ("trn" = String, Path, description = "Connection TRN identifier")
    ),
    request_body = ConnectionTestRequest,
    responses(
        (status = 200, description = "Connection test successful", body = crate::interface::dto::ExecuteResponseDto),
        (status = 400, description = "Invalid TRN format or test request", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
/// Test a connection by performing a simple GET to the given endpoint
pub async fn test(
    Path(trn): Path<String>,
    Json(req): Json<ConnectionTestRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    let exists = match svc.get_connection(&trn).await {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };
    if !exists {
        return helpers::not_found_error("connection").into_response();
    }
    let req = crate::interface::dto::AdhocExecuteRequestDto {
        connection_trn: trn,
        method: "GET".to_string(),
        endpoint: req.endpoint,
        headers: None,
        query: None,
        body: None,
        timeout_config: None,
        network_config: None,
        http_policy: None,
        response_policy: None,
        retry_policy: None,
    };
    match svc.execute_adhoc(req).await {
        Ok(res) => Json(serde_json::json!({
            "status": res.status,
            "headers": res.headers,
            "body": res.body,
        })).into_response(),
        Err(e) => helpers::execution_error(e.to_string()).into_response(),
    }
}
