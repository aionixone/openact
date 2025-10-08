//! Tenant extraction middleware

use axum::{
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Layer that extracts tenant
#[derive(Clone)]
pub struct TenantLayer;

impl<S> Layer<S> for TenantLayer {
    type Service = TenantService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TenantService { inner }
    }
}

/// Service that extracts tenant
#[derive(Clone)]
pub struct TenantService<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for TenantService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        // Extract tenant from header or query param
        let header_tenant = req.headers().get("x-tenant").and_then(|v| v.to_str().ok());
        let query_tenant = req.uri().query().and_then(|q| {
            // naive parser: split by '&' then '='
            for pair in q.split('&') {
                let mut it = pair.splitn(2, '=');
                let k = it.next().unwrap_or("");
                let v = it.next().unwrap_or("");
                if k == "tenant" || k == "x-tenant" {
                    return Some(v.to_string());
                }
            }
            None
        });

        let require = std::env::var("OPENACT_REQUIRE_TENANT")
            .map(|v| {
                let v = v.to_ascii_lowercase();
                v == "1" || v == "true" || v == "yes"
            })
            .unwrap_or(false);

        let tenant_opt = header_tenant.map(|s| s.to_string()).or(query_tenant);

        if tenant_opt.is_none() && require {
            // Build error response with request_id if available
            let request_id = req
                .extensions()
                .get::<crate::middleware::request_id::RequestId>()
                .map(|r| r.0.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let err = crate::error::ErrorResponse {
                success: false,
                error: crate::error::ErrorDetails {
                    code: "INVALID_INPUT".into(),
                    message: "Missing tenant (provide X-Tenant header)".into(),
                    details: Some(
                        json!({"hint": "Set X-Tenant header or disable OPENACT_REQUIRE_TENANT"}),
                    ),
                },
                metadata: crate::dto::ResponseMeta {
                    request_id,
                    tenant: None,
                    execution_time_ms: None,
                    action_trn: None,
                    version: None,
                    warnings: None,
                },
            };
            let resp = (StatusCode::BAD_REQUEST, Json(err)).into_response();
            let fut = async move { Ok(resp) };
            return Box::pin(fut);
        }

        let tenant = tenant_opt.unwrap_or_else(|| "default".to_string());
        req.extensions_mut().insert(Tenant(tenant));

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

/// Tenant extractor
#[derive(Clone)]
pub struct Tenant(pub String);

impl Tenant {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
