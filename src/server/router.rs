#![cfg(feature = "server")]

use axum::{Router, routing::{get, post}};

pub fn core_api_router() -> Router {
    Router::new()
        .route("/api/v1/connections", get(crate::server::handlers::connections::list).post(crate::server::handlers::connections::create))
        .route("/api/v1/connections/{trn..}", get(crate::server::handlers::connections::get).put(crate::server::handlers::connections::update).delete(crate::server::handlers::connections::del))
        .route("/api/v1/tasks", get(crate::server::handlers::tasks::list).post(crate::server::handlers::tasks::create))
        .route("/api/v1/tasks/{trn..}", get(crate::server::handlers::tasks::get).put(crate::server::handlers::tasks::update).delete(crate::server::handlers::tasks::del))
        .route("/api/v1/tasks/{trn..}/execute", post(crate::server::handlers::execute::execute))
        .route("/api/v1/system/stats", get(crate::server::handlers::system::stats))
        .route("/api/v1/system/cleanup", post(crate::server::handlers::system::cleanup))
}


