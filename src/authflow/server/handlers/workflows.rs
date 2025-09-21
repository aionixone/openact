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
pub async fn list_workflows(State(state): State<ServerState>) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();
    let workflow_list: Vec<_> = workflows.values().cloned().collect();
    Json(json!({ "workflows": workflow_list }))
}

#[cfg(feature = "server")]
pub async fn create_workflow(
    State(state): State<ServerState>,
    Json(req): Json<super::super::CreateWorkflowRequest>,
) -> impl IntoResponse {
    let normalized_json = crate::authflow::server::utils::normalize_dsl_json(req.dsl);
    let parsed: crate::authflow::dsl::OpenactDsl = match serde_json::from_value(normalized_json) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"code": "INVALID_DSL", "message": format!("Failed to parse normalized DSL: {}", e)}})),
            )
                .into_response();
        }
    };
    if let Err(e) = parsed.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"code": "VALIDATION_ERROR", "message": format!("DSL validation failed: {}", e)}})),
        )
            .into_response();
    }
    let workflow_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now();
    let workflow = super::super::WorkflowConfig {
        id: workflow_id.clone(),
        name: req.name,
        description: req.description,
        dsl: parsed,
        status: super::super::WorkflowStatus::Active,
        created_at: now,
        updated_at: now,
    };
    state.workflows.write().unwrap().insert(workflow_id.clone(), workflow.clone());
    (StatusCode::CREATED, Json(serde_json::to_value(workflow).unwrap())).into_response()
}

#[cfg(feature = "server")]
pub async fn get_workflow(State(state): State<ServerState>, Path(id): Path<String>) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();
    match workflows.get(&id) {
        Some(workflow) => Json(serde_json::to_value(workflow).unwrap()).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({
            "error": {"code": "WORKFLOW_NOT_FOUND", "message": "Workflow not found"}
        }))).into_response(),
    }
}

#[cfg(feature = "server")]
pub async fn get_workflow_graph(State(state): State<ServerState>, Path(id): Path<String>) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();
    let workflow = match workflows.get(&id) { Some(w) => w, None => {
        return (StatusCode::NOT_FOUND, Json(json!({
            "error": {"code": "WORKFLOW_NOT_FOUND", "message": "Workflow not found"}
        }))).into_response();
    } };
    let mut graphs = serde_json::Map::new();
    for (flow_name, flow) in &workflow.dsl.provider.flows {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut x_pos = 100;
        for (state_name, state) in &flow.states {
            let state_type = match state {
                stepflow_dsl::State::Task(_) => "task",
                stepflow_dsl::State::Choice(_) => "choice",
                stepflow_dsl::State::Wait(_) => "wait",
                stepflow_dsl::State::Succeed(_) => "succeed",
                stepflow_dsl::State::Fail(_) => "fail",
                stepflow_dsl::State::Pass(_) => "pass",
                stepflow_dsl::State::Parallel(_) => "parallel",
                stepflow_dsl::State::Map(_) => "map",
            };
            let resource = match state { stepflow_dsl::State::Task(task_state) => task_state.resource.as_str(), _ => "" };
            let node = json!({
                "id": state_name,
                "type": state_type,
                "label": state_name,
                "resource": resource,
                "position": { "x": x_pos, "y": 100 },
                "properties": { "description": format!("{} state", state_type), "canPause": matches!(state_type, "task") }
            });
            nodes.push(node); x_pos += 200;
        }
        for (state_name, state) in &flow.states {
            let next_state = match state {
                stepflow_dsl::State::Task(task_state) => task_state.base.next.as_deref(),
                stepflow_dsl::State::Pass(pass_state) => pass_state.base.next.as_deref(),
                stepflow_dsl::State::Wait(wait_state) => wait_state.base.next.as_deref(),
                stepflow_dsl::State::Choice(choice_state) => choice_state.choices.first().map(|c| c.next.as_str()),
                stepflow_dsl::State::Parallel(_) => None,
                stepflow_dsl::State::Map(_) => None,
                stepflow_dsl::State::Succeed(_) => None,
                stepflow_dsl::State::Fail(_) => None,
            };
            if let Some(next) = next_state { edges.push(json!({ "id": format!("{}_{}", state_name, next), "source": state_name, "target": next, "type": "success", "label": "success" })); }
        }
        graphs.insert(flow_name.clone(), json!({ "nodes": nodes, "edges": edges }));
    }
    Json(json!({ "workflowId": id, "graphs": graphs })).into_response()
}

#[cfg(feature = "server")]
pub async fn validate_workflow(State(state): State<ServerState>, Path(id): Path<String>) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();
    let workflow = match workflows.get(&id) { Some(w) => w, None => {
        return (StatusCode::NOT_FOUND, Json(json!({
            "error": {"code": "WORKFLOW_NOT_FOUND", "message": "Workflow not found"}
        }))).into_response();
    } };
    let validation_result = workflow.dsl.validate();
    match validation_result {
        Ok(_) => {
            let mut total_states = 0; let mut task_states = 0; let mut choice_states = 0; let mut end_states = 0;
            for flow in workflow.dsl.provider.flows.values() {
                for state in flow.states.values() {
                    total_states += 1;
                    match state {
                        stepflow_dsl::State::Task(_) => task_states += 1,
                        stepflow_dsl::State::Choice(_) => choice_states += 1,
                        stepflow_dsl::State::Succeed(_) | stepflow_dsl::State::Fail(_) => { end_states += 1 }
                        _ => {}
                    }
                }
            }
            Json(json!({
                "valid": true, "errors": [], "warnings": [],
                "statistics": { "totalStates": total_states, "taskStates": task_states, "choiceStates": choice_states, "endStates": end_states, "flowCount": workflow.dsl.provider.flows.len() }
            })).into_response()
        }
        Err(e) => Json(json!({
            "valid": false,
            "errors": [{ "code": "VALIDATION_ERROR", "message": e.to_string(), "path": "dsl" }],
            "warnings": [],
            "statistics": { "totalStates": 0, "taskStates": 0, "choiceStates": 0, "endStates": 0, "flowCount": 0 }
        })).into_response(),
    }
}


