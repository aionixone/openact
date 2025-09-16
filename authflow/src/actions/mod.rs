use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::engine::TaskHandler;
use crate::store::{ConnectionStore, Connection};

mod connection;
mod ensure;
mod http_request;
mod inject;
mod secrets;
pub mod multi_value;
mod oauth2 {
    pub mod client_credentials;
    pub mod refresh_token;
    pub mod authorize;
}
mod compute {
    pub mod hmac;
    pub mod jwt_sign;
    pub mod sigv4;
}

// Re-export existing handlers from current engine modules to provide a stable actions facade
pub use crate::actions::connection::{
    ConnectionContext, ConnectionReadHandler, ConnectionUpdateHandler,
};
pub use crate::actions::ensure::EnsureFreshTokenHandler;
pub use crate::actions::http_request::HttpTaskHandler;
pub use crate::actions::inject::{InjectApiKeyHandler, InjectBearerHandler, InjectBasicAuthHandler};
pub use crate::actions::oauth2::{
    authorize::OAuth2AuthorizeRedirectHandler,
    client_credentials::OAuth2ClientCredentialsHandler,
    refresh_token::OAuth2RefreshTokenHandler,
    authorize::OAuth2AwaitCallbackHandler,
};
#[cfg(feature = "vault")]
pub use crate::actions::secrets::VaultSecretsProvider;
pub use crate::actions::secrets::{
    MemorySecretsProvider, SecretsProvider, SecretsResolveHandler, SecretsResolveManyHandler,
};
pub use crate::actions::multi_value::{
    HttpPolicy, MultiValue, MultiValueMap, MultiValueMerger, MergeStrategy,
};

// A lightweight default router that wires common stateless actions.
// Stateful actions (e.g., connection.*, ensure.*) should be provided by a custom router.
#[derive(Clone)]
pub struct DefaultRouter;


impl TaskHandler for DefaultRouter {
    fn execute(&self, resource: &str, state_name: &str, ctx: &Value) -> Result<Value> {
        match resource {
            // HTTP
            "http.request" => HttpTaskHandler.execute(resource, state_name, ctx),

            // OAuth2
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

            // Inject
            "inject.bearer" => InjectBearerHandler.execute(resource, state_name, ctx),
            "inject.api_key" => InjectApiKeyHandler.execute(resource, state_name, ctx),
            "inject.basic" => InjectBasicAuthHandler.execute(resource, state_name, ctx),

            // Secrets (explicitly choose memory provider by default)
            "secrets.resolve" => SecretsResolveHandler::<MemorySecretsProvider>::default()
                .execute(resource, state_name, ctx),
            "secrets.resolve_many" => SecretsResolveManyHandler::<MemorySecretsProvider>::default()
                .execute(resource, state_name, ctx),

            // Compute
            "compute.hmac" => {
                crate::actions::compute::hmac::ComputeHmacHandler.execute(resource, state_name, ctx)
            }
            "compute.jwt_sign" => crate::actions::compute::jwt_sign::ComputeJwtSignHandler
                .execute(resource, state_name, ctx),
            "compute.sigv4" => crate::actions::compute::sigv4::ComputeSigV4Handler
                .execute(resource, state_name, ctx),

            // Connection operations - temporarily return error with helpful message
            "connection.read" | "connection.update" => {
                anyhow::bail!("Connection operations require a connection store. Use a custom router with ConnectionStore support.")
            }
            
            // Explicitly unsupported in default router to avoid hidden state deps  
            "ensure.fresh_token" => {
                anyhow::bail!("stateful action '{resource}' requires a custom router")
            }

            _ => anyhow::bail!("unknown resource {resource}"),
        }
    }
}

/// Pluggable router that composes stateless defaults and stateful handlers using an injected store
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
            // Stateful: connection.*
            "connection.read" => {
                let cref = ctx
                    .get("connection_ref")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("connection_ref required"))?;
                let val = futures::executor::block_on(self.connection_store.get(cref))?;
                Ok(serde_json::to_value(val.unwrap_or_default())?)
            }
            "connection.update" => {
                // Accept either a provided TRN via connection_ref or construct from parts
                let tenant = ctx.get("tenant").and_then(|v| v.as_str()).unwrap_or("default");
                let provider = ctx.get("provider").and_then(|v| v.as_str()).unwrap_or("unknown");
                let user_id = ctx.get("user_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let access_token = ctx
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("access_token required"))?;

                let mut conn = Connection::new(tenant, provider, user_id, access_token)?;
                if let Some(v) = ctx.get("refresh_token").and_then(|v| v.as_str()) {
                    conn.update_refresh_token(Some(v.to_string()));
                }
                if let Some(v) = ctx.get("expires_at").and_then(|v| v.as_str()) {
                    if let Ok(dt) = v.parse::<DateTime<Utc>>() {
                        conn = conn.with_expires_at(dt);
                    }
                }
                if let Some(v) = ctx.get("expires_in").and_then(|v| v.as_i64()) {
                    conn = conn.with_expires_in(v);
                }
                if let Some(v) = ctx.get("expires").and_then(|v| v.as_i64()) { // alias
                    conn = conn.with_expires_in(v);
                }

                let trn_key = conn.connection_id();
                futures::executor::block_on(self.connection_store.put(&trn_key, &conn))?;
                let mut out = serde_json::to_value(&conn)?;
                if let Value::Object(ref mut map) = out {
                    map.insert("trn".to_string(), Value::String(trn_key));
                }
                Ok(out)
            }

            // Stateful: ensure.fresh_token
            "ensure.fresh_token" => {
                let cref = ctx
                    .get("connection_ref")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("connection_ref required"))?;
                let token_url = ctx.get("tokenUrl").and_then(|v| v.as_str());
                let client_id = ctx.get("clientId").and_then(|v| v.as_str());
                let client_secret = ctx.get("clientSecret").and_then(|v| v.as_str());
                let skew = ctx.get("skewSeconds").and_then(|v| v.as_i64()).unwrap_or(120);

                let mut conn = futures::executor::block_on(self.connection_store.get(cref))?
                    .unwrap_or_default();

                let now = Utc::now();
                let expiry = conn.expires_at.unwrap_or_else(|| now - chrono::Duration::seconds(1));
                let needs = expiry <= now + chrono::Duration::seconds(skew);

                if needs {
                    let refresh_token = conn
                        .refresh_token
                        .clone()
                        .ok_or_else(|| anyhow!("missing refresh_token in connection"))?;
                    let (token_url, client_id, client_secret) = match (token_url, client_id, client_secret) {
                        (Some(a), Some(b), Some(c)) => (a.to_string(), b.to_string(), c.to_string()),
                        _ => return Err(anyhow!("tokenUrl/clientId/clientSecret required to refresh")),
                    };
                    let handler = crate::actions::OAuth2RefreshTokenHandler;
                    let in_json = json!({
                        "tokenUrl": token_url,
                        "clientId": client_id,
                        "clientSecret": client_secret,
                        "refresh_token": refresh_token,
                    });
                    let out = handler.execute("oauth2.refresh_token", state_name, &in_json)?;
                    if let Some(at) = out.get("access_token").and_then(|v| v.as_str()) {
                        conn.update_access_token(at);
                    }
                    if let Some(rt) = out.get("refresh_token").and_then(|v| v.as_str()) {
                        conn.update_refresh_token(Some(rt.to_string()));
                    }
                    if let Some(ex) = out.get("expires_in").and_then(|v| v.as_i64()) {
                        conn.expires_at = Some(now + chrono::Duration::seconds(ex));
                    }
                    futures::executor::block_on(self.connection_store.put(cref, &conn))?;
                }

                Ok(serde_json::to_value(conn)?)
            }

            // Fallback to stateless default router
            _ => self.default_router.execute(resource, state_name, ctx),
        }
    }
}

