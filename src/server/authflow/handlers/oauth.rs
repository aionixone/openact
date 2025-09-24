#[cfg(feature = "server")]
use axum::extract::Query;
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Json};
#[cfg(feature = "server")]
use axum::{extract::State, http::StatusCode};
#[cfg(feature = "server")]
use serde::Deserialize;
#[cfg(feature = "server")]
use serde_json::json;
#[cfg(feature = "server")]
use std::time::SystemTime;

#[cfg(all(feature = "server", feature = "openapi"))]
use utoipa::ToSchema;

use crate::server::authflow::runtime::execute_workflow;
#[cfg(feature = "server")]
use crate::server::authflow::state::ExecutionStatus;
#[cfg(feature = "server")]
use crate::server::authflow::state::ServerState;

/// OAuth2 callback parameters received from authorization server
///
/// This structure represents the standard OAuth2 authorization code callback parameters
/// that are received when the user completes the authorization flow and is redirected
/// back to the application.
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(feature = "server", feature = "openapi"), schema(example = json!({
    "code": "auth_code_12345",
    "state": "random_state_67890", 
    "execution_id": "exec_abc123def456"
})))]
pub struct CallbackParams {
    /// Authorization code received from the OAuth2 provider
    #[cfg_attr(
        all(feature = "server", feature = "openapi"),
        schema(example = "auth_code_12345")
    )]
    code: Option<String>,
    /// State parameter to prevent CSRF attacks (should match the one sent in the authorization request)
    #[cfg_attr(
        all(feature = "server", feature = "openapi"),
        schema(example = "random_state_67890")
    )]
    state: Option<String>,
    /// Optional execution ID to directly target a specific AuthFlow execution
    #[cfg_attr(
        all(feature = "server", feature = "openapi"),
        schema(example = "exec_abc123def456")
    )]
    execution_id: Option<String>,
}

/// OAuth2 authorization callback endpoint
///
/// This endpoint receives OAuth2 authorization callbacks from external providers
/// and resumes the corresponding AuthFlow execution with the authorization code.
///
/// **Flow Process:**
/// 1. User is redirected here after authorizing with OAuth2 provider
/// 2. Server extracts `code` and `state` from query parameters  
/// 3. Finds the matching paused AuthFlow execution using the state
/// 4. Injects the authorization code into the execution context
/// 5. Resumes the workflow execution to exchange code for tokens
///
/// **State Matching Strategy:**
/// - If `execution_id` is provided, targets that specific execution
/// - If `state` is provided, searches for execution with matching state
/// - Otherwise, uses the most recently updated running/paused execution
///
/// **Security Notes:**
/// - The `state` parameter is critical for CSRF protection
/// - Only running or paused executions can be resumed
/// - Invalid state values will result in 400 Bad Request
#[cfg(feature = "server")]
#[cfg_attr(all(feature = "server", feature = "openapi"), utoipa::path(
    get,
    path = "/oauth/callback",
    operation_id = "authflow_oauth_callback",
    tag = "authflow",
    summary = "Handle OAuth2 authorization callback",
    description = "Processes OAuth2 authorization callbacks from external providers and resumes the corresponding AuthFlow execution with the received authorization code.",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from OAuth2 provider"),
        ("state" = Option<String>, Query, description = "State parameter for CSRF protection"),
        ("execution_id" = Option<String>, Query, description = "Optional execution ID to target specific execution")
    ),
    responses(
        (status = 200, description = "Callback processed successfully", body = serde_json::Value),
        (status = 400, description = "No matching execution found or invalid parameters", body = crate::interface::error::ApiError),
        (status = 404, description = "Execution not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn oauth_callback(
    State(state): State<ServerState>,
    Query(params): Query<CallbackParams>,
) -> impl IntoResponse {
    println!(
        "[callback] received params code={:?} state={:?} execution_id={:?}",
        params.code, params.state, params.execution_id
    );
    let exec_id = if let Some(eid) = params.execution_id.clone() {
        println!("[callback] using provided execution_id={}", eid);
        eid
    } else if let Some(state_token) = params.state.clone() {
        fn context_matches_state(ctx: &serde_json::Value, s: &str) -> bool {
            if ctx.get("state").and_then(|v| v.as_str()) == Some(s) {
                return true;
            }
            if let Some(inner) = ctx.get("context") {
                if context_matches_state(inner, s) {
                    return true;
                }
            }
            if let Some(obj) = ctx.get("states").and_then(|v| v.as_object()) {
                for (_name, st) in obj.iter() {
                    if st
                        .get("result")
                        .and_then(|r| r.get("state"))
                        .and_then(|v| v.as_str())
                        == Some(s)
                    {
                        return true;
                    }
                }
            }
            false
        }
        let executions = state.executions.read().unwrap();
        if let Some((k, _)) = executions
            .iter()
            .filter(|(_, e)| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .find(|(_, e)| {
                e.context
                    .as_ref()
                    .map(|c| context_matches_state(c, &state_token))
                    .unwrap_or(false)
            })
        {
            k.clone()
        } else {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": {"code": "NO_EXECUTION", "message": "No execution matches the provided state"} }))).into_response();
        }
    } else {
        let executions = state.executions.read().unwrap();
        if let Some((k, _)) = executions
            .iter()
            .filter(|(_, e)| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .max_by_key(|(_, e)| e.updated_at)
        {
            k.clone()
        } else {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": {"code": "NO_EXECUTION", "message": "No running or paused execution to resume"} }))).into_response();
        }
    };

    let code = params.code.clone().unwrap_or_default();
    let state_val = params.state.clone().unwrap_or_default();

    {
        let mut executions = state.executions.write().unwrap();
        if let Some(execution) = executions.get_mut(&exec_id) {
            execution.status = ExecutionStatus::Running;
            execution.updated_at = SystemTime::now();
            let mut new_ctx = execution.context.clone().unwrap_or_else(|| json!({}));
            if let serde_json::Value::Object(ref mut map) = new_ctx {
                map.insert("code".to_string(), json!(code));
                map.insert("state".to_string(), json!(state_val));
            }
            execution.context = Some(new_ctx);
        } else {
            return (StatusCode::NOT_FOUND, Json(json!({ "error": {"code": "EXECUTION_NOT_FOUND", "message": "Execution not found"} }))).into_response();
        }
    }

    let state_clone = state.clone();
    let eid = exec_id.clone();
    tokio::spawn(async move {
        execute_workflow(state_clone, eid).await;
    });

    (
        StatusCode::OK,
        Json(json!({ "message": "Callback accepted", "executionId": exec_id })),
    )
        .into_response()
}
