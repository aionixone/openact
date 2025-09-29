#[cfg(feature = "server")]
use crate::server::handlers::{executions, health, oauth, workflows, ws};
#[cfg(feature = "server")]
use crate::server::state::ServerState;
#[cfg(feature = "server")]
use axum::{
    routing::{get, post},
    Router,
};

#[cfg(feature = "server")]
pub fn create_router() -> Router {
    let state = ServerState::new();
    create_router_with_state(state)
}

#[cfg(feature = "server")]
pub async fn create_router_async() -> Router {
    let state = ServerState::from_env().await;
    create_router_with_state(state)
}

#[cfg(feature = "server")]
pub fn create_router_with_state(state: ServerState) -> Router {
    Router::new()
        // Workflow management (authflow namespace only)
        .route(
            "/api/v1/authflow/workflows",
            get(workflows::list_workflows).post(workflows::create_workflow),
        )
        .route("/api/v1/authflow/workflows/{id}", get(workflows::get_workflow))
        .route("/api/v1/authflow/workflows/{id}/graph", get(workflows::get_workflow_graph))
        .route("/api/v1/authflow/workflows/{id}/validate", post(workflows::validate_workflow))
        // Execution management (authflow namespace only)
        .route(
            "/api/v1/authflow/executions",
            get(executions::list_executions).post(executions::start_execution),
        )
        .route("/api/v1/authflow/executions/{id}", get(executions::get_execution))
        .route("/api/v1/authflow/executions/{id}/resume", post(executions::resume_execution))
        .route("/api/v1/authflow/executions/{id}/cancel", post(executions::cancel_execution))
        .route("/api/v1/authflow/executions/{id}/trace", get(executions::get_execution_trace))
        // WebSocket real-time updates (authflow namespace only)
        .route("/api/v1/authflow/ws/executions", get(ws::websocket_handler))
        // System management (authflow namespace only)
        .route("/api/v1/authflow/health", get(health::health_check))
        // OAuth2 callback endpoint (authflow namespace only)
        .route("/api/v1/authflow/callback", get(oauth::oauth_callback))
        // Top-level OAuth callback alias for compatibility
        .route("/oauth/callback", get(oauth::oauth_callback))
        .with_state(state)
}
