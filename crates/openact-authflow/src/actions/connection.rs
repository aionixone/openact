use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::engine::TaskHandler;
use openact_core::{store::AuthConnectionStore, AuthConnection};
use openact_store::memory::MemoryAuthConnectionStore;

#[derive(Clone, Default)]
pub struct ConnectionContext<S: AuthConnectionStore = MemoryAuthConnectionStore> {
    pub store: S,
}

impl<S: AuthConnectionStore> ConnectionContext<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

#[derive(Default)]
pub struct ConnectionReadHandler<S: AuthConnectionStore = MemoryAuthConnectionStore> {
    pub ctx: ConnectionContext<S>,
}

#[derive(Default)]
pub struct ConnectionUpdateHandler<S: AuthConnectionStore = MemoryAuthConnectionStore> {
    pub ctx: ConnectionContext<S>,
}

impl<S: AuthConnectionStore> TaskHandler for ConnectionReadHandler<S> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let cref = ctx
            .get("connection_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("connection_ref required"))?;
        let auth_ref = normalize_auth_ref(cref);
        let rt = futures::executor::block_on(self.ctx.store.get(&auth_ref))?;
        Ok(serde_json::to_value(rt.unwrap_or_default())?)
    }
}

impl<S: AuthConnectionStore> TaskHandler for ConnectionUpdateHandler<S> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let cref = ctx
            .get("connection_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("connection_ref required"))?;
        let (tenant, provider, user_id) = parse_connection_ref(cref);

        let auth_trn = build_auth_trn(&tenant, &provider, &user_id);

        let mut current = if let Some(existing) =
            futures::executor::block_on(self.ctx.store.get(&auth_trn))?
        {
            existing
        } else {
            AuthConnection::new(
                tenant.clone(),
                provider.clone(),
                user_id.clone(),
                ctx.get("access_token").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            )
        };

        // Ensure base identity fields stay in sync with connection_ref
        current.tenant = tenant.clone();
        current.provider = provider.clone();
        current.user_id = user_id.clone();
        current.trn = auth_trn.clone();

        if let Some(v) = ctx.get("access_token").and_then(|v| v.as_str()) {
            current.update_access_token(v.to_string());
        }
        if let Some(v) = ctx.get("refresh_token").and_then(|v| v.as_str()) {
            current.update_refresh_token(Some(v.to_string()));
        }
        if let Some(v) = parse_expires_at(ctx) {
            current.expires_at = Some(v);
        }
        if let Some(v) = ctx.get("token_type").and_then(|v| v.as_str()) {
            current.token_type = v.to_string();
        }
        if let Some(v) = ctx.get("scope").and_then(|v| v.as_str()) {
            current.scope = Some(v.to_string());
        }
        if let Some(extra) = ctx.get("extra") {
            current.extra = extra.clone();
        }

        if cref != auth_trn {
            let _ = futures::executor::block_on(self.ctx.store.delete(cref));
        }
        futures::executor::block_on(self.ctx.store.put(&auth_trn, &current))?;

        let mut resp = serde_json::to_value(&current)?;
        if let Value::Object(obj) = &mut resp {
            obj.insert("connection_ref".to_string(), Value::String(cref.to_string()));
            obj.insert("auth_ref".to_string(), Value::String(auth_trn.clone()));
        }
        Ok(resp)
    }
}

fn parse_connection_ref(cref: &str) -> (String, String, String) {
    let mut parts = cref.split(':');
    let tenant = parts.next().unwrap_or("default").to_string();
    let provider = parts.next().unwrap_or("unknown").to_string();
    let user_id = parts.collect::<Vec<_>>().join(":");
    let user_id = if user_id.is_empty() { "unknown".to_string() } else { user_id };
    (tenant, provider, user_id)
}

fn parse_expires_at(ctx: &Value) -> Option<DateTime<Utc>> {
    if let Some(v) = ctx.get("expires_at").and_then(|v| v.as_str()) {
        if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
            return Some(dt.with_timezone(&Utc));
        }
        if let Ok(dt) = v.parse::<DateTime<Utc>>() {
            return Some(dt);
        }
    }
    if let Some(v) = ctx.get("expires_in").and_then(|v| v.as_i64()) {
        return Some(Utc::now() + chrono::Duration::seconds(v));
    }
    None
}

fn build_auth_trn(tenant: &str, provider: &str, user_id: &str) -> String {
    format!("trn:openact:{}:auth/{}/{}", tenant, provider, user_id)
}

pub(crate) fn normalize_auth_ref(connection_ref: &str) -> String {
    if connection_ref.starts_with("trn:openact:") {
        connection_ref.to_string()
    } else {
        let (tenant, provider, user_id) = parse_connection_ref(connection_ref);
        build_auth_trn(&tenant, &provider, &user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openact_store::memory::MemoryAuthConnectionStore;
    use serde_json::json;

    #[test]
    fn connection_update_creates_and_returns_auth_connection() {
        let store = MemoryAuthConnectionStore::new();
        let handler = ConnectionUpdateHandler { ctx: ConnectionContext::new(store.clone()) };
        let ctx = json!({
            "connection_ref": "tenant1:github:alice",
            "access_token": "tok",
            "refresh_token": "ref",
            "expires_in": 3600,
            "token_type": "Bearer",
            "scope": "repo"
        });

        let resp =
            handler.execute("connection.update", "Persist", &ctx).expect("update should succeed");

        assert_eq!(resp["tenant"], json!("tenant1"));
        assert_eq!(resp["provider"], json!("github"));
        assert_eq!(resp["user_id"], json!("alice"));
        assert_eq!(resp["trn"], json!("trn:openact:tenant1:auth/github/alice"));
        assert_eq!(resp["auth_ref"], json!("trn:openact:tenant1:auth/github/alice"));

        let stored =
            futures::executor::block_on(store.get("trn:openact:tenant1:auth/github/alice"))
                .expect("store get")
                .expect("auth connection stored");
        assert_eq!(stored.access_token, "tok");
        assert_eq!(stored.refresh_token, Some("ref".to_string()));
        assert_eq!(stored.scope, Some("repo".to_string()));
        assert_eq!(stored.token_type, "Bearer");
    }
}
