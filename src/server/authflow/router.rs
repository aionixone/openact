#[cfg(feature = "server")]
use axum::{Router, routing::{get, post}};
#[cfg(feature = "server")]
use crate::authflow::server::ServerState;

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
            get(crate::authflow::server::handlers::workflows::list_workflows)
                .post(crate::authflow::server::handlers::workflows::create_workflow),
        )
        .route(
            "/api/v1/authflow/workflows/{id}",
            get(crate::authflow::server::handlers::workflows::get_workflow),
        )
        .route(
            "/api/v1/authflow/workflows/{id}/graph",
            get(crate::authflow::server::handlers::workflows::get_workflow_graph),
        )
        .route(
            "/api/v1/authflow/workflows/{id}/validate",
            post(crate::authflow::server::handlers::workflows::validate_workflow),
        )
        // Execution management (authflow namespace only)
        .route(
            "/api/v1/authflow/executions",
            get(crate::authflow::server::handlers::executions::list_executions)
                .post(crate::authflow::server::handlers::executions::start_execution),
        )
        .route(
            "/api/v1/authflow/executions/{id}",
            get(crate::authflow::server::handlers::executions::get_execution),
        )
        .route(
            "/api/v1/authflow/executions/{id}/resume",
            post(crate::authflow::server::handlers::executions::resume_execution),
        )
        .route(
            "/api/v1/authflow/executions/{id}/cancel",
            post(crate::authflow::server::handlers::executions::cancel_execution),
        )
        .route(
            "/api/v1/authflow/executions/{id}/trace",
            get(crate::authflow::server::handlers::executions::get_execution_trace),
        )
        // WebSocket real-time updates (authflow namespace only)
        .route(
            "/api/v1/authflow/ws/executions",
            get(crate::authflow::server::handlers::ws::websocket_handler),
        )
        // System management (authflow namespace only)
        .route(
            "/api/v1/authflow/health",
            get(crate::authflow::server::handlers::health::health_check),
        )
        // OAuth2 callback endpoint (authflow namespace only)
        .route(
            "/api/v1/authflow/callback",
            get(crate::authflow::server::handlers::oauth::oauth_callback),
        )
        .with_state(state)
}


