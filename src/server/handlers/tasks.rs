#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::TaskUpsertRequest;
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListQuery {
    pub connection_trn: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/tasks",
    tag = "tasks",
    operation_id = "tasks_list",
    summary = "List tasks",
    description = "Retrieve a list of tasks with optional filtering",
    params(
        ("connection_trn" = Option<String>, Query, description = "Filter by connection TRN"),
        ("limit" = Option<i64>, Query, description = "Maximum number of tasks to return"),
        ("offset" = Option<i64>, Query, description = "Number of tasks to skip for pagination")
    ),
    responses(
        (status = 200, description = "List of tasks", body = Vec<crate::models::TaskConfig>),
        (status = 400, description = "Invalid query parameters", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn list(
    State(svc): State<OpenActService>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    match svc
        .list_tasks(q.connection_trn.as_deref(), q.limit, q.offset)
        .await
    {
        Ok(list) => Json(serde_json::json!(list)).into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/tasks",
    tag = "tasks",
    operation_id = "tasks_create",
    summary = "Create task",
    description = "Create a new task configuration",
    request_body = TaskUpsertRequest,
    responses(
        (status = 201, description = "Task created successfully", body = crate::models::TaskConfig),
        (status = 400, description = "Invalid task data", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn create(
    State(svc): State<OpenActService>,
    Json(req): Json<TaskUpsertRequest>,
) -> impl IntoResponse {
    // Validate TRN format
    use crate::utils::trn::parse_task_trn;
    if let Err(e) = parse_task_trn(&req.trn) {
        return helpers::validation_error("invalid_input", format!("Invalid TRN format: {}", e))
            .into_response();
    }

    // Validate connection TRN format
    use crate::utils::trn::parse_connection_trn;
    if let Err(e) = parse_connection_trn(&req.connection_trn) {
        return helpers::validation_error(
            "invalid_input",
            format!("Invalid connection TRN format: {}", e),
        )
        .into_response();
    }

    // Convert DTO to config with metadata (new creation)
    let config = req.to_config(None, None);

    match svc.upsert_task(&config).await {
        Ok(_) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!(config)),
        )
            .into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/tasks/{trn}",
    tag = "tasks",
    operation_id = "tasks_get",
    summary = "Get task by TRN",
    description = "Retrieve a specific task configuration by its TRN",
    params(
        ("trn" = String, Path, description = "Task TRN identifier")
    ),
    responses(
        (status = 200, description = "Task found", body = crate::models::TaskConfig),
        (status = 400, description = "Invalid TRN format", body = crate::interface::error::ApiError),
        (status = 404, description = "Task not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn get(State(svc): State<OpenActService>, Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    match svc.get_task(&trn).await {
        Ok(Some(task)) => Json(serde_json::json!(task)).into_response(),
        Ok(None) => helpers::not_found_error("task").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/v1/tasks/{trn}",
    tag = "tasks",
    operation_id = "tasks_update",
    summary = "Update task",
    description = "Update an existing task configuration",
    params(
        ("trn" = String, Path, description = "Task TRN identifier")
    ),
    request_body = TaskUpsertRequest,
    responses(
        (status = 200, description = "Task updated successfully", body = crate::models::TaskConfig),
        (status = 400, description = "Invalid TRN format or data", body = crate::interface::error::ApiError),
        (status = 404, description = "Task not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn update(
    State(svc): State<OpenActService>,
    Path(trn): Path<String>,
    Json(req): Json<TaskUpsertRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    if req.trn != trn {
        return helpers::validation_error("trn_mismatch", "trn mismatch").into_response();
    }

    // Get existing version and created_at for proper versioning
    let (existing_version, existing_created_at) = match svc.get_task(&trn).await {
        Ok(Some(existing)) => (Some(existing.version), Some(existing.created_at)),
        Ok(None) => (None, None), // Treat as creation if doesn't exist
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };

    // Convert DTO to config with proper versioning
    let config = req.to_config(existing_version, existing_created_at);

    match svc.upsert_task(&config).await {
        Ok(_) => Json(serde_json::json!(config)).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/v1/tasks/{trn}",
    tag = "tasks",
    operation_id = "tasks_delete",
    summary = "Delete task",
    description = "Delete a task configuration",
    params(
        ("trn" = String, Path, description = "Task TRN identifier")
    ),
    responses(
        (status = 204, description = "Task deleted successfully"),
        (status = 400, description = "Invalid TRN format", body = crate::interface::error::ApiError),
        (status = 404, description = "Task not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn del(State(svc): State<OpenActService>, Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    match svc.delete_task(&trn).await {
        Ok(true) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Ok(false) => helpers::not_found_error("task").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}
