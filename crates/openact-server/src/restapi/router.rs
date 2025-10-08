//! REST API router

use crate::{
    middleware::{RequestIdLayer, TenantLayer},
    AppState,
};
use axum::{routing::get, Router};
use openact_mcp::GovernanceConfig;
use tower::ServiceBuilder;

/// Create REST API router
pub fn create_router() -> Router<(AppState, GovernanceConfig)> {
    let base = Router::new()
        .route("/api/v1/kinds", get(super::handlers::kinds::get_kinds))
        .route("/api/v1/actions", get(super::handlers::actions::get_actions))
        .route("/api/v1/actions/:action/schema", get(super::handlers::actions::get_action_schema))
        .route(
            "/api/v1/actions/:action/execute",
            axum::routing::post(super::handlers::actions::execute_action),
        )
        .route(
            "/api/v1/actions/:action/execute/stream",
            axum::routing::post(super::handlers::actions::execute_action_stream),
        )
        .route(
            "/api/v1/execute-inline",
            axum::routing::post(super::handlers::actions::execute_inline),
        )
        .route("/api/v1/execute", axum::routing::post(super::handlers::actions::execute_by_trn))
        .route("/api/v1/health", get(super::handlers::health::health_check))
        .layer(ServiceBuilder::new().layer(TenantLayer).layer(RequestIdLayer));

    // Optionally mount authflow router when feature enabled AND runtime flag set
    #[cfg(feature = "authflow")]
    let base = {
        let enable_authflow = std::env::var("OPENACT_ENABLE_AUTHFLOW")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true); // Default to enabled when feature is compiled in

        if enable_authflow {
            let base = base
                .route(
                    "/api/v1/authflow/runs",
                    axum::routing::post(super::handlers::authflow::start_flow_run),
                )
                .route(
                    "/api/v1/authflow/runs/:run_id",
                    axum::routing::get(super::handlers::authflow::get_flow_run),
                );

            // Build authflow router from the embedded server module (has its own state)
            let authflow_router = openact_authflow::server::router::create_router();

            // Mount authflow router directly - axum 0.7 should handle different state types
            // The authflow routes are already absolute paths, so mount at root
            base.fallback_service(authflow_router)
        } else {
            tracing::info!("AuthFlow feature compiled but disabled by OPENACT_ENABLE_AUTHFLOW");
            base
        }
    };

    base
}
