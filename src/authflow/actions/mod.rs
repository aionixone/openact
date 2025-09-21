use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

use crate::authflow::engine::TaskHandler;
use crate::store::ConnectionStore;

mod connection;
mod ensure;
mod http_request;
mod inject;
mod secrets;
pub mod oauth2 {
    pub mod client_credentials;
    pub mod refresh_token;
    pub mod authorize;
}
pub mod compute {
    pub mod hmac;
    pub mod jwt_sign;
    pub mod sigv4;
}

// Re-export handlers from local subtree
pub use self::connection::{
    ConnectionContext, ConnectionReadHandler, ConnectionUpdateHandler,
};
pub use self::ensure::EnsureFreshTokenHandler;
pub use self::http_request::HttpTaskHandler;
pub use self::inject::{InjectApiKeyHandler, InjectBearerHandler};
pub use self::oauth2::{
    authorize::OAuth2AuthorizeRedirectHandler,
    client_credentials::OAuth2ClientCredentialsHandler,
    refresh_token::OAuth2RefreshTokenHandler,
    authorize::OAuth2AwaitCallbackHandler,
};
#[cfg(feature = "vault")]
pub use self::secrets::VaultSecretsProvider;
pub use self::secrets::{
    MemorySecretsProvider, SecretsProvider, SecretsResolveHandler, SecretsResolveManyHandler,
};

#[derive(Clone)]
pub struct DefaultRouter;

impl TaskHandler for DefaultRouter {
    fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
        match resource {
            "http.request" => HttpTaskHandler.execute(resource, state_name, ctx),
            "oauth2.client_credentials" => {
                OAuth2ClientCredentialsHandler.execute(resource, state_name, ctx)
            }
            "oauth2.refresh_token" => OAuth2RefreshTokenHandler.execute(resource, state_name, ctx),
            "oauth2.authorize_redirect" => {
                OAuth2AuthorizeRedirectHandler.execute(resource, state_name, ctx)
            }
            "oauth2.await_callback" => {
                OAuth2AwaitCallbackHandler.execute(resource, state_name, ctx)
            }
            "inject.bearer" => InjectBearerHandler.execute(resource, state_name, ctx),
            "inject.api_key" => InjectApiKeyHandler.execute(resource, state_name, ctx),
            "secrets.resolve" => SecretsResolveHandler::<MemorySecretsProvider>::default()
                .execute(resource, state_name, ctx),
            "secrets.resolve_many" => SecretsResolveManyHandler::<MemorySecretsProvider>::default()
                .execute(resource, state_name, ctx),
            "compute.hmac" => {
                self::compute::hmac::ComputeHmacHandler.execute(resource, state_name, ctx)
            }
            "compute.jwt_sign" => self::compute::jwt_sign::ComputeJwtSignHandler
                .execute(resource, state_name, ctx),
            "compute.sigv4" => self::compute::sigv4::ComputeSigV4Handler
                .execute(resource, state_name, ctx),
            "connection.read" | "connection.update" => {
                anyhow::bail!("Connection operations require a connection store. Use a custom router with ConnectionStore support.")
            }
            "ensure.fresh_token" => {
                anyhow::bail!("stateful action '{resource}' requires a custom router")
            }
            _ => anyhow::bail!("unknown resource {resource}"),
        }
    }
}

#[derive(Clone)]
pub struct ActionRouter {
    pub default_router: DefaultRouter,
    pub connection_store: Arc<dyn ConnectionStore>,
}

impl ActionRouter {
    pub fn new(connection_store: Arc<dyn ConnectionStore>) -> Self {
        Self { default_router: DefaultRouter, connection_store }
    }
}

impl TaskHandler for ActionRouter {
    fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
        match resource {
            "connection.read" => {
                // Use Memory store for now to satisfy generic bound
                let ctx_wrap = ConnectionContext::new(crate::store::MemoryConnectionStore::new());
                ConnectionReadHandler { ctx: ctx_wrap }.execute(resource, state_name, ctx)
            }
            "connection.update" => {
                let ctx_wrap = ConnectionContext::new(crate::store::MemoryConnectionStore::new());
                ConnectionUpdateHandler { ctx: ctx_wrap }.execute(resource, state_name, ctx)
            }
            "ensure.fresh_token" => {
                let handler = EnsureFreshTokenHandler { store: Arc::new(crate::store::MemoryConnectionStore::new()) };
                handler.execute(resource, state_name, ctx)
            }
            _ => self.default_router.execute(resource, state_name, ctx),
        }
    }
}

