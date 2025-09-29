//! Request ID middleware

use axum::{http::Request, response::Response};
use std::task::{Context, Poll};
use tower::{Layer, Service};
use uuid::Uuid;

/// Layer that adds request IDs
#[derive(Clone)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Service that adds request IDs
#[derive(Clone)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for RequestIdService<S>
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
        let request_id = if let Some(existing_id) =
            req.headers().get("x-request-id").and_then(|v| v.to_str().ok())
        {
            existing_id.to_string()
        } else {
            let id = Uuid::new_v4().to_string();
            req.headers_mut().insert("x-request-id", id.parse().unwrap());
            id
        };

        // Store in extensions for handlers to access
        req.extensions_mut().insert(RequestId(request_id.clone()));

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            response.headers_mut().insert("x-request-id", request_id.parse().unwrap());
            Ok(response)
        })
    }
}

/// Request ID extractor
#[derive(Clone)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
