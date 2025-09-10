use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::engine::TaskHandler;
use crate::store::{ConnectionStore, MemoryConnectionStore};

#[derive(Clone, Default)]
pub struct ConnectionContext<S: ConnectionStore = MemoryConnectionStore> {
    pub store: S,
}

impl<S: ConnectionStore> ConnectionContext<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

#[derive(Default)]
pub struct ConnectionReadHandler<S: ConnectionStore = MemoryConnectionStore> {
    pub ctx: ConnectionContext<S>,
}

#[derive(Default)]
pub struct ConnectionUpdateHandler<S: ConnectionStore = MemoryConnectionStore> {
    pub ctx: ConnectionContext<S>,
}

impl<S: ConnectionStore> TaskHandler for ConnectionReadHandler<S> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let cref = ctx
            .get("connection_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("connection_ref required"))?;
        let rt = futures::executor::block_on(self.ctx.store.get(cref))?;
        Ok(serde_json::to_value(rt.unwrap_or_default())?)
    }
}

impl<S: ConnectionStore> TaskHandler for ConnectionUpdateHandler<S> {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        let cref = ctx
            .get("connection_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("connection_ref required"))?;
        let mut current =
            futures::executor::block_on(self.ctx.store.get(cref))?.unwrap_or_default();
        if let Some(v) = ctx.get("access_token").and_then(|v| v.as_str()) {
            current.update_access_token(v);
        }
        if let Some(v) = ctx.get("refresh_token").and_then(|v| v.as_str()) {
            current.update_refresh_token(Some(v.to_string()));
        }
        if let Some(v) = ctx.get("expires_at").and_then(|v| v.as_str()) {
            let dt: DateTime<Utc> = v.parse().unwrap_or_else(|_| Utc::now());
            current.expires_at = Some(dt);
        }
        if let Some(v) = ctx.get("expires_in").and_then(|v| v.as_i64()) {
            current.expires_at = Some(Utc::now() + chrono::Duration::seconds(v));
        }
        futures::executor::block_on(self.ctx.store.put(cref, &current))?;
        Ok(serde_json::to_value(current)?)
    }
}
