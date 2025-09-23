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

#[cfg(feature = "server")]
use crate::authflow::server::ServerState;
#[cfg(feature = "server")]
use crate::authflow::server::ExecutionStatus;
use crate::authflow::server::runtime::execute_workflow;

#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    execution_id: Option<String>,
}

#[cfg(feature = "server")]
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
            if ctx.get("state").and_then(|v| v.as_str()) == Some(s) { return true; }
            if let Some(inner) = ctx.get("context") { if context_matches_state(inner, s) { return true; } }
            if let Some(obj) = ctx.get("states").and_then(|v| v.as_object()) {
                for (_name, st) in obj.iter() {
                    if st.get("result").and_then(|r| r.get("state")).and_then(|v| v.as_str()) == Some(s) { return true; }
                }
            }
            false
        }
        let executions = state.executions.read().unwrap();
        if let Some((k, _)) = executions
            .iter()
            .filter(|(_, e)| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .find(|(_, e)| e.context.as_ref().map(|c| context_matches_state(c, &state_token)).unwrap_or(false))
        { k.clone() } else {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": {"code": "NO_EXECUTION", "message": "No execution matches the provided state"} }))).into_response();
        }
    } else {
        let executions = state.executions.read().unwrap();
        if let Some((k, _)) = executions
            .iter()
            .filter(|(_, e)| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .max_by_key(|(_, e)| e.updated_at) { k.clone() } else {
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
    tokio::spawn(async move { execute_workflow(state_clone, eid).await; });

    (StatusCode::OK, Json(json!({ "message": "Callback accepted", "executionId": exec_id }))).into_response()
}



