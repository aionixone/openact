//! REST API router

use crate::{
    middleware::{RequestIdLayer, TenantLayer},
    AppState,
};
use axum::{routing::get, Router};
use openact_mcp::GovernanceConfig;
use tower::ServiceBuilder;

/// Create REST API router
pub fn create_router(app_state: AppState, governance: GovernanceConfig) -> Router {
    Router::new()
        .route("/api/v1/kinds", get(super::handlers::kinds::get_kinds))
        .route(
            "/api/v1/actions",
            get(super::handlers::actions::get_actions),
        )
        .route(
            "/api/v1/actions/:action/schema",
            get(super::handlers::actions::get_action_schema),
        )
        .route(
            "/api/v1/actions/:action/execute",
            axum::routing::post(super::handlers::actions::execute_action),
        )
        .route(
            "/api/v1/execute",
            axum::routing::post(super::handlers::actions::execute_by_trn),
        )
        .route("/api/v1/health", get(super::handlers::health::health_check))
        .layer(
            ServiceBuilder::new()
                .layer(TenantLayer)
                .layer(RequestIdLayer),
        )
        .with_state((app_state, governance))
}
