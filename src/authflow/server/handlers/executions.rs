#[cfg(feature = "server")]
use axum::extract::{Path, State};
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Json};
#[cfg(feature = "server")]
use axum::http::StatusCode;
#[cfg(feature = "server")]
use serde_json::json;

#[cfg(feature = "server")]
use crate::authflow::server::ServerState;
#[cfg(feature = "server")]
use super::super::ExecutionStatus;

/// Get specific execution information
#[cfg(feature = "server")]
pub async fn get_execution(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let executions = state.executions.read().unwrap();

    match executions.get(&id) {
        Some(execution) => {
            let mut response = serde_json::to_value(execution).unwrap();
            // Add pending_info if execution is paused and has context
            if matches!(execution.status, ExecutionStatus::Paused) && execution.context.is_some() {
                if let Some(context) = &execution.context {
                    // Extract authorization URL from context if available
                    let mut pending = serde_json::Map::new();
                    if let Some(auth_result) = context.pointer("/states/StartAuth/result") {
                        if let Some(authorize_url) = auth_result.get("authorize_url").and_then(|v| v.as_str()) {
                            pending.insert("authorize_url".to_string(), serde_json::Value::String(authorize_url.to_string()));
                        }
                        if let Some(state_val) = auth_result.get("state").and_then(|v| v.as_str()) {
                            pending.insert("state".to_string(), serde_json::Value::String(state_val.to_string()));
                        }
                        if let Some(verifier) = auth_result.get("code_verifier").and_then(|v| v.as_str()) {
                            pending.insert("code_verifier".to_string(), serde_json::Value::String(verifier.to_string()));
                        }
                    }
                    // Fallback to vars if needed (state/code_verifier saved via assign)
                    if let Some(vars) = context.get("vars") {
                        if !pending.contains_key("state") {
                            if let Some(state_val) = vars.get("auth_state").and_then(|v| v.as_str()) {
                                pending.insert("state".to_string(), serde_json::Value::String(state_val.to_string()));
                            }
                        }
                        if !pending.contains_key("code_verifier") {
                            if let Some(verifier) = vars.get("code_verifier").and_then(|v| v.as_str()) {
                                pending.insert("code_verifier".to_string(), serde_json::Value::String(verifier.to_string()));
                            }
                        }
                    }
                    if !pending.is_empty() {
                        if let Some(response_obj) = response.as_object_mut() {
                            response_obj.insert("pending_info".to_string(), serde_json::Value::Object(pending));
                        }
                    }
                }
            }
            Json(response).into_response()
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "EXECUTION_NOT_FOUND",
                    "message": "Execution not found"
                }
            })),
        )
            .into_response(),
    }
}

/// Get execution trace
#[cfg(feature = "server")]
pub async fn get_execution_trace(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let executions = state.executions.read().unwrap();

    let execution = match executions.get(&id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": {
                        "code": "EXECUTION_NOT_FOUND",
                        "message": "Execution not found"
                    }
                })),
            )
                .into_response();
        }
    };

    // Build trace data
    let trace: Vec<_> = execution.state_history.iter().map(|entry| {
        json!({
            "state": entry.state,
            "status": entry.status,
            "enteredAt": entry.entered_at,
            "exitedAt": entry.exited_at,
            "transition": if entry.exited_at.is_some() { serde_json::Value::String("success".to_string()) } else { serde_json::Value::Null }
        })
    }).collect();

    // Add current state (if running or paused)
    let mut current_trace = trace;
    if let Some(current_state) = &execution.current_state {
        if matches!(execution.status, ExecutionStatus::Running | ExecutionStatus::Paused) {
            current_trace.push(json!({
                "state": current_state,
                "status": "active",
                "enteredAt": execution.updated_at,
                "exitedAt": serde_json::Value::Null,
                "transition": serde_json::Value::Null
            }));
        }
    }

    Json(json!({
        "executionId": id,
        "trace": current_trace,
        "currentState": execution.current_state,
        "status": execution.status,
        "nextPossibleStates": []
    }))
    .into_response()
}

/// List executions
#[cfg(feature = "server")]
pub async fn list_executions(State(state): State<ServerState>) -> impl IntoResponse {
    let executions = state.executions.read().unwrap();
    let execution_list: Vec<_> = executions.values().cloned().collect();

    Json(json!({ "executions": execution_list }))
}

/// Start workflow execution
#[cfg(feature = "server")]
pub async fn start_execution(
    State(state): State<ServerState>,
    Json(req): Json<super::super::StartExecutionRequest>,
) -> impl IntoResponse {
    // Check if workflow exists
    let workflow = {
        let workflows = state.workflows.read().unwrap();
        match workflows.get(&req.workflow_id) {
            Some(w) => w.clone(),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "error": { "code": "WORKFLOW_NOT_FOUND", "message": "Workflow not found" }
                    })),
                )
                    .into_response();
            }
        }
    };

    // Check if flow exists
    if !workflow.dsl.provider.flows.contains_key(&req.flow) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": { "code": "FLOW_NOT_FOUND", "message": format!("Flow '{}' not found in workflow", req.flow) }
            })),
        )
            .into_response();
    }

    let execution_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now();

    let execution = super::super::ExecutionInfo {
        execution_id: execution_id.clone(),
        workflow_id: req.workflow_id.clone(),
        flow: req.flow.clone(),
        status: ExecutionStatus::Running,
        current_state: None,
        started_at: now,
        updated_at: now,
        completed_at: None,
        input: req.input.clone(),
        context: req.context,
        error: None,
        state_history: Vec::new(),
    };

    {
        let mut executions = state.executions.write().unwrap();
        executions.insert(execution_id.clone(), execution.clone());
    }

    // Start asynchronous execution
    let state_clone = state.clone();
    let execution_id_clone = execution_id.clone();
    tokio::spawn(async move { crate::authflow::server::runtime::execute_workflow(state_clone, execution_id_clone).await; });

    (StatusCode::CREATED, Json(serde_json::to_value(execution).unwrap())).into_response()
}

/// Resume paused execution
#[cfg(feature = "server")]
pub async fn resume_execution(
    State(state): State<ServerState>,
    Path(id): Path<String>,
    Json(req): Json<super::super::ResumeExecutionRequest>,
) -> impl IntoResponse {
    // Check if execution exists (allow Running/Paused, prohibit Completed/Failed/Cancelled)
    {
        let executions = state.executions.read().unwrap();
        match executions.get(&id) {
            Some(execution) => {
                if matches!(
                    execution.status,
                    ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled
                ) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": {
                                "code": "EXECUTION_NOT_RESUMABLE",
                                "message": "Execution is finished and cannot be resumed"
                            }
                        })),
                    )
                        .into_response();
                }
            }
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "error": {
                            "code": "EXECUTION_NOT_FOUND",
                            "message": "Execution not found"
                        }
                    })),
                )
                    .into_response();
            }
        }
    }

    // Write code/state to the top level of context for oauth2.await_callback to read
    {
        let mut executions = state.executions.write().unwrap();
        if let Some(execution) = executions.get_mut(&id) {
            execution.status = ExecutionStatus::Running;
            execution.updated_at = std::time::SystemTime::now();
            let mut new_ctx = execution.context.clone().unwrap_or_else(|| json!({}));
            if let serde_json::Value::Object(ref mut map) = new_ctx {
                if let Some(code) = req.input.get("code").cloned() { map.insert("code".to_string(), code); }
                if let Some(state_val) = req.input.get("state").cloned() { map.insert("state".to_string(), state_val); }
            }
            execution.context = Some(new_ctx);
        }
    }

    // Start asynchronous resume execution
    let state_clone = state.clone();
    let execution_id_clone = id.clone();
    tokio::spawn(async move { crate::authflow::server::runtime::execute_workflow(state_clone, execution_id_clone).await; });

    Json(json!({ "message": "Execution resumed", "executionId": id })).into_response()
}

/// Cancel execution
#[cfg(feature = "server")]
pub async fn cancel_execution(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut executions = state.executions.write().unwrap();

    match executions.get_mut(&id) {
        Some(execution) => {
            if matches!(
                execution.status,
                ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled
            ) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": {
                            "code": "EXECUTION_ALREADY_FINISHED",
                            "message": "Execution is already finished"
                        }
                    })),
                )
                    .into_response();
            }

            execution.status = ExecutionStatus::Cancelled;
            execution.updated_at = std::time::SystemTime::now();
            execution.completed_at = Some(std::time::SystemTime::now());

            Json(json!({ "message": "Execution cancelled", "executionId": id })).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "EXECUTION_NOT_FOUND",
                    "message": "Execution not found"
                }
            })),
        )
            .into_response(),
    }
}


