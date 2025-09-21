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
        // Workflow management
        .route(
            "/api/v1/workflows",
            get(crate::authflow::server::handlers::workflows::list_workflows)
                .post(crate::authflow::server::handlers::workflows::create_workflow),
        )
        .route(
            "/api/v1/workflows/{id}",
            get(crate::authflow::server::handlers::workflows::get_workflow),
        )
        .route(
            "/api/v1/workflows/{id}/graph",
            get(crate::authflow::server::handlers::workflows::get_workflow_graph),
        )
        .route(
            "/api/v1/workflows/{id}/validate",
            post(crate::authflow::server::handlers::workflows::validate_workflow),
        )
        // Execution management
        .route(
            "/api/v1/executions",
            get(crate::authflow::server::handlers::executions::list_executions)
                .post(crate::authflow::server::handlers::executions::start_execution),
        )
        .route(
            "/api/v1/executions/{id}",
            get(crate::authflow::server::handlers::executions::get_execution),
        )
        .route(
            "/api/v1/executions/{id}/resume",
            post(crate::authflow::server::handlers::executions::resume_execution),
        )
        .route(
            "/api/v1/executions/{id}/cancel",
            post(crate::authflow::server::handlers::executions::cancel_execution),
        )
        .route(
            "/api/v1/executions/{id}/trace",
            get(crate::authflow::server::handlers::executions::get_execution_trace),
        )
        // WebSocket real-time updates
        .route(
            "/api/v1/ws/executions",
            get(crate::authflow::server::handlers::ws::websocket_handler),
        )
        // System management
        .route(
            "/api/v1/health",
            get(crate::authflow::server::handlers::health::health_check),
        )
        // OAuth2 callback endpoint (auto resume)
        .route(
            "/oauth/callback",
            get(crate::authflow::server::handlers::oauth::oauth_callback),
        )
        .with_state(state)
}


