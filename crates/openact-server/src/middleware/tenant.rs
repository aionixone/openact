//! Tenant extraction middleware

use axum::{http::Request, response::Response};
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
        // Extract tenant from X-Tenant header or query param, default to "default"
        let tenant = req
            .headers()
            .get("x-tenant")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("default")
            .to_string();

        // Store in extensions for handlers to access
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
