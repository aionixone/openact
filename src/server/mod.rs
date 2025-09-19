//! openact Web Server
//!
//! Provides REST API for workflow management and execution monitoring

#[cfg(feature = "server")]
use axum::extract::ws::{Message, WebSocket};
#[cfg(feature = "server")]
use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade, Query},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
#[cfg(feature = "server")]
use futures::{sink::SinkExt, stream::StreamExt};
#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "server")]
use serde_json::json;
#[cfg(feature = "server")]
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::SystemTime,
};
#[cfg(feature = "server")]
use tokio::sync::broadcast;
#[cfg(feature = "server")]
use uuid::Uuid;

#[cfg(feature = "server")]
use crate::{
    actions::{DefaultRouter, ActionRouter},
    dsl::OpenactDsl,
    engine::{RunOutcome, run_until_pause_or_end, TaskHandler},
    store::{
        ConnectionStore, MemoryConnectionStore, MemoryRunStore, StoreBackend, StoreConfig,
        create_connection_store,
    },
};
#[cfg(feature = "server")]
use stepflow_dsl;
#[cfg(feature = "server")]
use chrono::{DateTime, Utc};

/// Workflow server state
#[cfg(feature = "server")]
#[derive(Clone)]
pub struct ServerState {
    /// Workflow storage
    pub workflows: Arc<RwLock<HashMap<String, WorkflowConfig>>>,
    /// Execution storage
    pub executions: Arc<RwLock<HashMap<String, ExecutionInfo>>>,
    /// Connection storage
    pub connection_store: Arc<dyn ConnectionStore>,
    /// Run storage
    pub run_store: Arc<MemoryRunStore>,
    /// Task handler
    pub task_handler: Arc<dyn TaskHandler>,
    /// WebSocket broadcast channel
    pub ws_broadcaster: broadcast::Sender<ExecutionEvent>,
}

/// Execution event
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEvent {
    pub event_type: String,
    pub execution_id: String,
    pub timestamp: SystemTime,
    pub data: serde_json::Value,
}

/// Workflow configuration
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub dsl: OpenactDsl,
    pub status: WorkflowStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

/// Workflow status
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStatus {
    Active,
    Inactive,
    Draft,
}

/// Create workflow request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    // Accept raw JSON for normalization (parameters/inlineTemplate folding, provider.config injection)
    pub dsl: serde_json::Value,
}

/// Execution information
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInfo {
    pub execution_id: String,
    pub workflow_id: String,
    pub flow: String,
    pub status: ExecutionStatus,
    pub current_state: Option<String>,
    pub started_at: SystemTime,
    pub updated_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub input: serde_json::Value,
    pub context: Option<serde_json::Value>,
    pub error: Option<String>,
    pub state_history: Vec<StateHistoryEntry>,
}

/// Execution status
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// State history entry
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateHistoryEntry {
    pub state: String,
    pub status: String,
    pub entered_at: SystemTime,
    pub exited_at: Option<SystemTime>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Start execution request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartExecutionRequest {
    pub workflow_id: String,
    pub flow: String,
    pub input: serde_json::Value,
    pub context: Option<serde_json::Value>,
}

/// Resume execution request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeExecutionRequest {
    pub input: serde_json::Value,
}



#[cfg(feature = "server")]
impl ServerState {
    pub fn new() -> Self {
        let (ws_broadcaster, _) = broadcast::channel(1000);
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store: Arc::new(MemoryConnectionStore::new()) as Arc<dyn ConnectionStore>,
            run_store: Arc::new(MemoryRunStore::default()),
            task_handler: Arc::new(DefaultRouter) as Arc<dyn TaskHandler>,
            ws_broadcaster,
        }
    }

    /// Create from environment variables (supports switching storage backend)
    pub async fn from_env() -> Self {
        let (ws_broadcaster, _) = broadcast::channel(1000);
        // openact_STORE: memory | sqlite
        let store_env = std::env::var("openact_STORE").unwrap_or_else(|_| "memory".to_string());
        println!("[server] openact_STORE environment variable: {}", store_env);
        let mut backend = StoreBackend::Memory;
        
        if store_env.eq_ignore_ascii_case("sqlite") {
            backend = StoreBackend::Sqlite;
            println!("[server] Using SQLite backend");
        } else {
            println!("[server] Using Memory backend (sqlite feature enabled but openact_STORE != 'sqlite')");
        }
        // Note: sqlite feature flag removed; fallback to memory if not configured

        let mut cfg = StoreConfig {
            backend,
            ..Default::default()
        };
        
        if let Ok(db_url) = std::env::var("OPENACT_DATABASE_URL")
            .or_else(|_| std::env::var("openact_SQLITE_URL")) {
            use crate::store::sqlite_connection_store::SqliteConfig;
            cfg.sqlite = Some(SqliteConfig {
                database_url: db_url,
                ..Default::default()
            });
        }

        let connection_store = create_connection_store(cfg)
            .await
            .unwrap_or_else(|e| {
                eprintln!("[server] Failed to create connection store: {:?}", e);
                eprintln!("[server] Falling back to MemoryConnectionStore");
                Arc::new(MemoryConnectionStore::new()) as Arc<dyn ConnectionStore>
            });
        let router: Arc<dyn TaskHandler> = Arc::new(ActionRouter::new(connection_store.clone()));
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store,
            run_store: Arc::new(MemoryRunStore::default()),
            task_handler: router,
            ws_broadcaster,
        }
    }

    /// Send execution event
    pub fn broadcast_event(&self, event: ExecutionEvent) {
        let _ = self.ws_broadcaster.send(event);
    }
}

/// Create Web server router
#[cfg(feature = "server")]
pub fn create_router() -> Router {
    let state = ServerState::new();
    create_router_with_state(state)
}

/// Create Web server router (asynchronous version, supports environment variable configuration)
#[cfg(feature = "server")]
pub async fn create_router_async() -> Router {
    let state = ServerState::from_env().await;
    create_router_with_state(state)
}

/// Create router with specified state
#[cfg(feature = "server")]
pub fn create_router_with_state(state: ServerState) -> Router {
    Router::new()
        // Workflow management
        .route(
            "/api/v1/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route("/api/v1/workflows/{id}", get(get_workflow))
        .route("/api/v1/workflows/{id}/graph", get(get_workflow_graph))
        .route("/api/v1/workflows/{id}/validate", post(validate_workflow))
        // Execution management
        .route(
            "/api/v1/executions",
            get(list_executions).post(start_execution),
        )
        .route("/api/v1/executions/{id}", get(get_execution))
        .route("/api/v1/executions/{id}/resume", post(resume_execution))
        .route("/api/v1/executions/{id}/cancel", post(cancel_execution))
        .route("/api/v1/executions/{id}/trace", get(get_execution_trace))
        // WebSocket real-time updates
        .route("/api/v1/ws/executions", get(websocket_handler))
        // System management
        .route("/api/v1/health", get(health_check))
        // OAuth2 callback endpoint (auto resume)
        .route("/oauth/callback", get(oauth_callback))
        .with_state(state)
}

#[cfg(feature = "server")]
fn normalize_dsl_json(mut dsl: serde_json::Value) -> serde_json::Value {
    // 1) legacy -> new: Fold mapping.input.template into parameters (if parameters do not exist)
    if let Some(provider) = dsl.get_mut("provider") {
        if let Some(flows) = provider.get_mut("flows").and_then(|v| v.as_object_mut()) {
            for (_flow, flow_obj) in flows.iter_mut() {
                if let Some(states) = flow_obj.get_mut("states").and_then(|v| v.as_object_mut()) {
                    for (_sn, state) in states.iter_mut() {
                        // If parameters do not exist but mapping.input.template does, promote to parameters
                        let params_exists = state.get("parameters").is_some();
                        if !params_exists {
                            if let Some(mapping) = state.get_mut("mapping").and_then(|v| v.as_object_mut()) {
                                if let Some(input) = mapping.get_mut("input").and_then(|v| v.as_object_mut()) {
                                    // Compatible with inlineTemplate
                                    if let Some(inline) = input.remove("inlineTemplate") {
                                        input.insert("template".into(), inline);
                                    }
                                    if let Some(template) = input.remove("template") {
                                        if let Some(state_map) = state.as_object_mut() {
                                            state_map.insert("parameters".to_string(), template);
                                        }
                                    }
                                }
                            }
                        }
                        // Optional: Clean up legacy mapping structure to avoid ambiguity
                        if let Some(state_map) = state.as_object_mut() {
                            state_map.remove("mapping");
                        }
                    }
                }
            }
        }
    }

    // 2) Inject provider.config into each task's parameters (explicit fields take precedence, do not overwrite existing fields)
    let provider_config = dsl
        .get("provider")
        .and_then(|p| p.get("config"))
        .cloned()
        .unwrap_or(serde_json::json!({}));
    if provider_config.is_object() {
        if let Some(provider) = dsl.get_mut("provider") {
            if let Some(flows) = provider.get_mut("flows").and_then(|v| v.as_object_mut()) {
                for (_flow, flow_obj) in flows.iter_mut() {
                    if let Some(states) = flow_obj.get_mut("states").and_then(|v| v.as_object_mut()) {
                        for (_sn, state) in states.iter_mut() {
                            if let Some(params) = state.get_mut("parameters").and_then(|v| v.as_object_mut()) {
                                if let Some(cfg) = provider_config.as_object() {
                                    for (k, v) in cfg.iter() {
                                        if !params.contains_key(k) {
                                            params.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    dsl
}

#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    execution_id: Option<String>,
}

#[cfg(feature = "server")]
async fn oauth_callback(
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
        // Reverse lookup execution by state (compatible with context override where state only exists in states.*.result.state)
        fn context_matches_state(ctx: &serde_json::Value, s: &str) -> bool {
            // 1) Top-level state
            if ctx.get("state").and_then(|v| v.as_str()) == Some(s) { return true; }
            // 2) Nested context.state (after execution context wrapping)
            if let Some(inner) = ctx.get("context") {
                if context_matches_state(inner, s) { return true; }
            }
            // 3) Iterate over states.*.result.state
            if let Some(obj) = ctx.get("states").and_then(|v| v.as_object()) {
                for (_name, st) in obj.iter() {
                    if st.get("result").and_then(|r| r.get("state")).and_then(|v| v.as_str()) == Some(s) {
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
            .find(|(_, e)| e.context.as_ref().map(|c| context_matches_state(c, &state_token)).unwrap_or(false))
        {
            println!("[callback] matched execution by state token: {}", state_token);
            k.clone()
        } else {
            println!("[callback] NO_EXECUTION matched for state token: {}", state_token);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": {"code": "NO_EXECUTION", "message": "No execution matches the provided state"} })),
            )
                .into_response();
        }
    } else {
        // If execution_id is not explicitly provided and state is not provided, choose the latest Running/Paused execution
        let executions = state.executions.read().unwrap();
        if let Some((k, _)) = executions
            .iter()
            .filter(|(_, e)| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .max_by_key(|(_, e)| e.updated_at)
        {
            println!("[callback] fallback to latest running/paused execution: {}", k);
            k.clone()
        } else {
            println!("[callback] NO_EXECUTION: no running/paused execution found");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": {"code": "NO_EXECUTION", "message": "No running or paused execution to resume"} })),
            )
                .into_response();
        }
    };

    let code = params.code.clone().unwrap_or_default();
    let state_val = params.state.clone().unwrap_or_default();

    // Write code/state and trigger resume
    {
        let mut executions = state.executions.write().unwrap();
        if let Some(execution) = executions.get_mut(&exec_id) {
            println!(
                "[callback] resuming execution={} with code(len={}) state={}",
                exec_id,
                code.len(),
                state_val
            );
            execution.status = ExecutionStatus::Running;
            execution.updated_at = SystemTime::now();
            let mut new_ctx = execution.context.clone().unwrap_or_else(|| json!({}));
            if let serde_json::Value::Object(ref mut map) = new_ctx {
                map.insert("code".to_string(), json!(code));
                map.insert("state".to_string(), json!(state_val));
            }
            execution.context = Some(new_ctx);
        } else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": {"code": "EXECUTION_NOT_FOUND", "message": "Execution not found"} })),
            )
                .into_response();
        }
    }

    let state_clone = state.clone();
    let eid = exec_id.clone();
    tokio::spawn(async move {
        println!("[callback] spawning execute_workflow for execution={}", eid);
        execute_workflow(state_clone, eid).await;
    });

    (StatusCode::OK, Json(json!({ "message": "Callback accepted", "executionId": exec_id }))).into_response()
}

// ==================== Workflow Management API ====================

/// List workflows
#[cfg(feature = "server")]
async fn list_workflows(State(state): State<ServerState>) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();
    let workflow_list: Vec<_> = workflows.values().cloned().collect();

    Json(json!({
        "workflows": workflow_list
    }))
}

/// Create workflow
#[cfg(feature = "server")]
async fn create_workflow(
    State(state): State<ServerState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> impl IntoResponse {
    // Normalize DSL (JSON layer transformation), then deserialize to strong type and validate
    let normalized_json = normalize_dsl_json(req.dsl);
    let parsed: OpenactDsl = match serde_json::from_value(normalized_json) {
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
            Json(json!({
                "error": {
                    "code": "VALIDATION_ERROR",
                    "message": format!("DSL validation failed: {}", e)
                }
            })),
        )
            .into_response();
    }

    let workflow_id = Uuid::new_v4().to_string();
    let now = SystemTime::now();

    let workflow = WorkflowConfig {
        id: workflow_id.clone(),
        name: req.name,
        description: req.description,
        dsl: parsed,
        status: WorkflowStatus::Active,
        created_at: now,
        updated_at: now,
    };

    state
        .workflows
        .write()
        .unwrap()
        .insert(workflow_id.clone(), workflow.clone());

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(workflow).unwrap()),
    )
        .into_response()
}

/// Get specific workflow
#[cfg(feature = "server")]
async fn get_workflow(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();

    match workflows.get(&id) {
        Some(workflow) => Json(serde_json::to_value(workflow).unwrap()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "WORKFLOW_NOT_FOUND",
                    "message": "Workflow not found"
                }
            })),
        )
            .into_response(),
    }
}

/// Get workflow graph structure
#[cfg(feature = "server")]
async fn get_workflow_graph(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();

    let workflow = match workflows.get(&id) {
        Some(w) => w,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": {
                        "code": "WORKFLOW_NOT_FOUND",
                        "message": "Workflow not found"
                    }
                })),
            )
                .into_response();
        }
    };

    // Generate graph structure for each flow
    let mut graphs = serde_json::Map::new();

    for (flow_name, flow) in &workflow.dsl.provider.flows {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut x_pos = 100;

        // Generate nodes
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

            let resource = match state {
                stepflow_dsl::State::Task(task_state) => task_state.resource.as_str(),
                _ => "",
            };

            let node = json!({
                "id": state_name,
                "type": state_type,
                "label": state_name,
                "resource": resource,
                "position": { "x": x_pos, "y": 100 },
                "properties": {
                    "description": format!("{} state", state_type),
                    "canPause": matches!(state_type, "task"),
                }
            });
            nodes.push(node);
            x_pos += 200;
        }

        // Generate edges (simplified version)
        for (state_name, state) in &flow.states {
            let next_state = match state {
                stepflow_dsl::State::Task(task_state) => task_state.base.next.as_deref(),
                stepflow_dsl::State::Pass(pass_state) => pass_state.base.next.as_deref(),
                stepflow_dsl::State::Wait(wait_state) => wait_state.base.next.as_deref(),
                stepflow_dsl::State::Choice(choice_state) => {
                    // Choice state has multiple branches, simplified here
                    choice_state.choices.first().map(|c| c.next.as_str())
                }
                stepflow_dsl::State::Parallel(_) => None,
                stepflow_dsl::State::Map(_) => None,
                _ => None,
            };

            if let Some(next) = next_state {
                let edge = json!({
                    "id": format!("{}_{}", state_name, next),
                    "source": state_name,
                    "target": next,
                    "type": "success",
                    "label": "success"
                });
                edges.push(edge);
            }
        }

        graphs.insert(
            flow_name.clone(),
            json!({
                "nodes": nodes,
                "edges": edges
            }),
        );
    }

    Json(json!({
        "workflowId": id,
        "graphs": graphs
    }))
    .into_response()
}

/// Validate workflow configuration
#[cfg(feature = "server")]
async fn validate_workflow(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let workflows = state.workflows.read().unwrap();

    let workflow = match workflows.get(&id) {
        Some(w) => w,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": {
                        "code": "WORKFLOW_NOT_FOUND",
                        "message": "Workflow not found"
                    }
                })),
            )
                .into_response();
        }
    };

    // Perform DSL validation
    let validation_result = workflow.dsl.validate();

    match validation_result {
        Ok(_) => {
            let mut total_states = 0;
            let mut task_states = 0;
            let mut choice_states = 0;
            let mut end_states = 0;

            for flow in workflow.dsl.provider.flows.values() {
                for state in flow.states.values() {
                    total_states += 1;
                    match state {
                        stepflow_dsl::State::Task(_) => task_states += 1,
                        stepflow_dsl::State::Choice(_) => choice_states += 1,
                        stepflow_dsl::State::Succeed(_) | stepflow_dsl::State::Fail(_) => {
                            end_states += 1
                        }
                        _ => {}
                    }
                }
            }

            Json(json!({
                "valid": true,
                "errors": [],
                "warnings": [],
                "statistics": {
                    "totalStates": total_states,
                    "taskStates": task_states,
                    "choiceStates": choice_states,
                    "endStates": end_states,
                    "flowCount": workflow.dsl.provider.flows.len()
                }
            }))
            .into_response()
        }
        Err(e) => Json(json!({
            "valid": false,
            "errors": [{
                "code": "VALIDATION_ERROR",
                "message": e.to_string(),
                "path": "dsl"
            }],
            "warnings": [],
            "statistics": {
                "totalStates": 0,
                "taskStates": 0,
                "choiceStates": 0,
                "endStates": 0,
                "flowCount": 0
            }
        }))
        .into_response(),
    }
}

// ==================== Execution Management API ====================

/// List executions
#[cfg(feature = "server")]
async fn list_executions(State(state): State<ServerState>) -> impl IntoResponse {
    let executions = state.executions.read().unwrap();
    let execution_list: Vec<_> = executions.values().cloned().collect();

    Json(json!({
        "executions": execution_list
    }))
}

/// Start workflow execution
#[cfg(feature = "server")]
async fn start_execution(
    State(state): State<ServerState>,
    Json(req): Json<StartExecutionRequest>,
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
                        "error": {
                            "code": "WORKFLOW_NOT_FOUND",
                            "message": "Workflow not found"
                        }
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
                "error": {
                    "code": "FLOW_NOT_FOUND",
                    "message": format!("Flow '{}' not found in workflow", req.flow)
                }
            })),
        )
            .into_response();
    }

    let execution_id = Uuid::new_v4().to_string();
    let now = SystemTime::now();

    // Create execution information
    let execution = ExecutionInfo {
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

    // Store execution information
    state
        .executions
        .write()
        .unwrap()
        .insert(execution_id.clone(), execution.clone());

    // Broadcast start event
    state.broadcast_event(ExecutionEvent {
        event_type: "execution_started".to_string(),
        execution_id: execution_id.clone(),
        timestamp: now,
        data: json!({
            "status": "running",
            "workflowId": req.workflow_id,
            "flow": req.flow,
            "input": req.input
        }),
    });

    // If it's an OAuth2 authorization flow, attempt to construct and broadcast authorization link (for easy front-end/user access)
    if let (Some(auth_url_v), Some(client_id_v), Some(redirect_uri_v)) = (
        workflow.dsl.get_provider_config("authorizeUrl"),
        workflow.dsl.get_provider_config("clientId"),
        workflow.dsl.get_provider_config("redirectUri"),
    ) {
        if let (Some(auth_url), Some(client_id), Some(redirect_uri)) = (
            auth_url_v.as_str(),
            client_id_v.as_str(),
            redirect_uri_v.as_str(),
        ) {
            let scope = workflow
                .dsl
                .get_provider_config("defaultScope")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let state_str = uuid::Uuid::new_v4().to_string();
            let mut url = format!(
                "{}?response_type=code&client_id={}&redirect_uri={}&state={}",
                auth_url,
                urlencoding::encode(client_id),
                urlencoding::encode(redirect_uri),
                urlencoding::encode(&state_str),
            );
            if !scope.is_empty() {
                url.push_str(&format!("&scope={}", urlencoding::encode(scope)));
            }
            state.broadcast_event(ExecutionEvent {
                event_type: "oauth2_authorize".to_string(),
                execution_id: execution_id.clone(),
                timestamp: now,
                data: json!({
                    "authorizeUrl": url,
                    "state": state_str
                }),
            });
            // Also write to execution context
            {
                let mut executions = state.executions.write().unwrap();
                if let Some(exec) = executions.get_mut(&execution_id) {
                    exec.context = Some(json!({
                        "authorizeUrl": url,
                        "state": state_str
                    }));
                }
            }
        }
    }

    // Start asynchronous execution
    let state_clone = state.clone();
    let execution_id_clone = execution_id.clone();
    tokio::spawn(async move {
        execute_workflow(state_clone, execution_id_clone).await;
    });

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(execution).unwrap()),
    )
        .into_response()
}

/// Get specific execution information
#[cfg(feature = "server")]
async fn get_execution(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let executions = state.executions.read().unwrap();

    match executions.get(&id) {
        Some(execution) => Json(serde_json::to_value(execution).unwrap()).into_response(),
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

/// Resume paused execution
#[cfg(feature = "server")]
async fn resume_execution(
    State(state): State<ServerState>,
    Path(id): Path<String>,
    Json(req): Json<ResumeExecutionRequest>,
) -> impl IntoResponse {
    // Check if execution exists (allow Running/Paused, prohibit Completed/Failed/Cancelled)
    {
        let executions = state.executions.read().unwrap();
        match executions.get(&id) {
            Some(execution) => {
                if matches!(
                    execution.status,
                    ExecutionStatus::Completed
                        | ExecutionStatus::Failed
                        | ExecutionStatus::Cancelled
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

    // Update execution status to Running
    {
        let mut executions = state.executions.write().unwrap();
        if let Some(execution) = executions.get_mut(&id) {
            execution.status = ExecutionStatus::Running;
            execution.updated_at = SystemTime::now();
            // Write code/state to the top level of context for oauth2.await_callback to read
            let mut new_ctx = execution.context.clone().unwrap_or_else(|| json!({}));
            if let serde_json::Value::Object(ref mut map) = new_ctx {
                if let Some(code) = req.input.get("code").cloned() {
                    map.insert("code".to_string(), code);
                }
                if let Some(state_val) = req.input.get("state").cloned() {
                    map.insert("state".to_string(), state_val);
                }
            }
            execution.context = Some(new_ctx);
        }
    }

    // Start asynchronous resume execution
    let state_clone = state.clone();
    let execution_id_clone = id.clone();
    tokio::spawn(async move {
        execute_workflow(state_clone, execution_id_clone).await;
    });

    Json(json!({
        "message": "Execution resumed",
        "executionId": id
    }))
    .into_response()
}

/// Cancel execution
#[cfg(feature = "server")]
async fn cancel_execution(
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
            execution.updated_at = SystemTime::now();
            execution.completed_at = Some(SystemTime::now());

            Json(json!({
                "message": "Execution cancelled",
                "executionId": id
            }))
            .into_response()
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

/// Asynchronous workflow execution
#[cfg(feature = "server")]
async fn execute_workflow(state: ServerState, execution_id: String) {
    let (workflow, flow_name, input, context) = {
        let executions = state.executions.read().unwrap();
        let execution = match executions.get(&execution_id) {
            Some(e) => e,
            None => return,
        };

        let workflows = state.workflows.read().unwrap();
        let workflow = match workflows.get(&execution.workflow_id) {
            Some(w) => w.clone(),
            None => return,
        };

        (
            workflow,
            execution.flow.clone(),
            execution.input.clone(),
            execution.context.clone(),
        )
    };

    // Build execution context
    let mut exec_context = serde_json::Map::new();
    exec_context.insert("input".to_string(), input);
    
    // Inject provider configuration into context for template expressions to access
    exec_context.insert("provider".to_string(), json!({
        "config": workflow.dsl.provider.config
    }));
    
    // Merge context content directly to the top level to avoid nesting
    if let Some(ctx) = context {
        if let serde_json::Value::Object(ctx_map) = ctx {
            for (k, v) in ctx_map {
                exec_context.insert(k, v);
            }
        } else {
            // If context is not an object, retain the original nested structure
            exec_context.insert("context".to_string(), ctx);
        }
    }
    let exec_context = serde_json::Value::Object(exec_context);

    // Get flow definition
    let flow = match workflow.dsl.provider.flows.get(&flow_name) {
        Some(f) => f,
        None => {
            // Update execution status to Failed
            let mut executions = state.executions.write().unwrap();
            if let Some(execution) = executions.get_mut(&execution_id) {
                execution.status = ExecutionStatus::Failed;
                execution.error = Some(format!("Flow '{}' not found", flow_name));
                execution.updated_at = SystemTime::now();
                execution.completed_at = Some(SystemTime::now());
            }
            return;
        }
    };

    // Execute workflow
    let start_state = {
        let executions = state.executions.read().unwrap();
        if let Some(execution) = executions.get(&execution_id) {
            execution.current_state.as_deref().unwrap_or(&flow.start_at).to_string()
        } else {
            flow.start_at.clone()
        }
    };
    println!(
        "[engine] execute_workflow start: execution={} flow={} start_at={}",
        execution_id,
        flow_name,
        start_state
    );
    
    let result = run_until_pause_or_end(
        flow,
        &start_state,
        exec_context,
        state.task_handler.as_ref(),
        100, // Maximum steps
    );

    // Update execution status and broadcast event
    let mut executions = state.executions.write().unwrap();
    if let Some(execution) = executions.get_mut(&execution_id) {
        let now = SystemTime::now();
        execution.updated_at = now;

        match result {
            Ok(RunOutcome::Finished(final_context)) => {
                execution.status = ExecutionStatus::Completed;
                execution.completed_at = Some(now);
                execution.context = Some(final_context.clone());

                // Broadcast completion event
                state.broadcast_event(ExecutionEvent {
                    event_type: "execution_completed".to_string(),
                    execution_id: execution_id.clone(),
                    timestamp: now,
                    data: json!({
                        "status": "completed",
                        "context": final_context
                    }),
                });
            }
            Ok(RunOutcome::Pending(pending_info)) => {
                // Handle pending state, waiting for subsequent resume
                execution.status = ExecutionStatus::Paused;
                execution.current_state = Some(pending_info.next_state);
                // Update context
                execution.context = Some(pending_info.context);
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("PAUSE_FOR_CALLBACK") {
                    execution.status = ExecutionStatus::Paused;
                    state.broadcast_event(ExecutionEvent {
                        event_type: "execution_paused".to_string(),
                        execution_id: execution_id.clone(),
                        timestamp: now,
                        data: json!({ "reason": "await_callback" }),
                    });
                } else {
                    execution.status = ExecutionStatus::Failed;
                    execution.error = Some(msg.clone());
                    execution.completed_at = Some(now);

                    // Broadcast failure event
                    state.broadcast_event(ExecutionEvent {
                        event_type: "execution_failed".to_string(),
                        execution_id: execution_id.clone(),
                        timestamp: now,
                        data: json!({
                            "status": "failed",
                            "error": msg
                        }),
                    });
                }
            }
        }
    }
}

/// Get execution trace
#[cfg(feature = "server")]
async fn get_execution_trace(
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
        if matches!(
            execution.status,
            ExecutionStatus::Running | ExecutionStatus::Paused
        ) {
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
        "nextPossibleStates": [] // Can be generated based on DSL analysis
    }))
    .into_response()
}

// ==================== WebSocket Real-time Updates ====================

/// WebSocket handler
#[cfg(feature = "server")]
async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<ServerState>) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle WebSocket connection
#[cfg(feature = "server")]
async fn handle_websocket(socket: WebSocket, state: ServerState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_receiver = state.ws_broadcaster.subscribe();

    // Start event broadcast task
    let broadcast_task = tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            let message = match serde_json::to_string(&event) {
                Ok(json) => Message::Text(json.into()),
                Err(_) => continue,
            };

            if sender.send(message).await.is_err() {
                break;
            }
        }
    });

    // Handle client messages
    let ping_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Handle client messages (e.g., ping/pong)
                    if text == "ping" {
                        // Can send pong response here
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Wait for tasks to complete
    tokio::select! {
        _ = broadcast_task => {},
        _ = ping_task => {},
    }
}

// ==================== System Management API ====================

/// Health check
#[cfg(feature = "server")]
async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "version": "1.0.0",
        "timestamp": SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}

// Placeholder when server feature is not enabled
#[cfg(not(feature = "server"))]
pub fn create_router() -> () {
    panic!("Server feature is not enabled. Please compile with --features server");
}
