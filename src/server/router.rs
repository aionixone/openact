#![cfg(feature = "server")]

use axum::{
    Router,
    routing::{get, post},
};
use crate::app::service::OpenActService;

#[cfg(feature = "openapi")]
use utoipa_swagger_ui::SwaggerUi;

/// Create router with injected service state
pub fn core_api_router_with_state(service: OpenActService) -> Router {
    // Ensure background tasks started (idempotent)
    crate::server::init_background_tasks();

    #[cfg_attr(not(feature = "openapi"), allow(unused_mut))]
    let mut router = Router::new()
        .route(
            "/api/v1/connections",
            get(crate::server::handlers::connections::list)
                .post(crate::server::handlers::connections::create),
        )
        .route(
            "/api/v1/connections/{trn..}",
            get(crate::server::handlers::connections::get)
                .put(crate::server::handlers::connections::update)
                .delete(crate::server::handlers::connections::del),
        )
        .route(
            "/api/v1/connections/{trn..}/status",
            get(crate::server::handlers::connections::status),
        )
        .route(
            "/api/v1/connections/{trn..}/test",
            post(crate::server::handlers::connections::test),
        )
        .route(
            "/api/v1/tasks",
            get(crate::server::handlers::tasks::list).post(crate::server::handlers::tasks::create),
        )
        .route(
            "/api/v1/tasks/{trn..}",
            get(crate::server::handlers::tasks::get)
                .put(crate::server::handlers::tasks::update)
                .delete(crate::server::handlers::tasks::del),
        )
        .route(
            "/api/v1/tasks/{trn..}/execute",
            post(crate::server::handlers::execute::execute),
        )
        .route(
            "/api/v1/execute/adhoc",
            post(crate::server::handlers::execute::execute_adhoc),
        )
        // Connect flows
        .route(
            "/api/v1/connect",
            post(crate::server::handlers::connect::connect),
        )
        .route(
            "/api/v1/connect/ac/resume",
            post(crate::server::handlers::connect::connect_ac_resume),
        )
        .route(
            "/api/v1/connect/ac/status",
            get(crate::server::handlers::connect::connect_ac_status),
        )
        .route(
            "/api/v1/connect/device-code",
            post(crate::server::handlers::connect::connect_device_code),
        )
        .route(
            "/api/v1/system/health",
            get(crate::server::handlers::system::health),
        )
        .route(
            "/api/v1/system/stats",
            get(crate::server::handlers::system::stats),
        )
        .route(
            "/api/v1/system/cleanup",
            post(crate::server::handlers::system::cleanup),
        )
        // Observability endpoints
        .route(
            "/health",
            get(crate::observability::endpoints::detailed_health),
        )
        .route(
            "/metrics",
            get(crate::observability::endpoints::metrics_endpoint),
        )
        .route("/debug", get(crate::observability::endpoints::debug_info));

    // Add OpenAPI documentation routes when the feature is enabled
    #[cfg(feature = "openapi")]
    {
        router = router.merge(SwaggerUi::new("/docs").url(
            "/api-docs/openapi.json",
            crate::api::openapi::get_openapi_spec(),
        ));
    }

    router.with_state(service)
}

/// Create router with service from environment (backward compatibility)
pub async fn core_api_router() -> Router {
    let service = OpenActService::from_env().await.expect("Failed to create OpenActService");
    core_api_router_with_state(service)
}
