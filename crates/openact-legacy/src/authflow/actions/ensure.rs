use anyhow::{Result, anyhow};
use chrono::{Duration, Utc};
use serde_json::{Value, json};

use crate::authflow::actions;
use crate::authflow::engine::TaskHandler;
use crate::store::ConnectionStore;
use std::sync::Arc;

#[derive(Clone)]
pub struct EnsureFreshTokenHandler {
    pub store: Arc<dyn ConnectionStore>,
}

impl TaskHandler for EnsureFreshTokenHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx: { connection_ref, tokenUrl, clientId, clientSecret, skewSeconds? }
        let cref = ctx
            .get("connection_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("connection_ref required"))?;
        let token_url = ctx.get("tokenUrl").and_then(|v| v.as_str());
        let client_id = ctx.get("clientId").and_then(|v| v.as_str());
        let client_secret = ctx.get("clientSecret").and_then(|v| v.as_str());
        let skew = ctx
            .get("skewSeconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(120);

        let mut conn = futures::executor::block_on(self.store.get(cref))?.unwrap_or_default();

        // decide if needs refresh
        let now = Utc::now();
        let expiry = conn
            .expires_at
            .unwrap_or_else(|| now - Duration::seconds(1)); // treat unknown as expired
        let needs = expiry <= now + Duration::seconds(skew);

        if needs {
            let refresh_token = conn
                .refresh_token
                .clone()
                .ok_or_else(|| anyhow!("missing refresh_token in connection"))?;
            let (token_url, client_id, client_secret) = match (token_url, client_id, client_secret)
            {
                (Some(a), Some(b), Some(c)) => (a.to_string(), b.to_string(), c.to_string()),
                _ => {
                    return Err(anyhow!(
                        "tokenUrl/clientId/clientSecret required to refresh"
                    ));
                }
            };
            let handler = actions::OAuth2RefreshTokenHandler;
            let in_json = json!({
                "tokenUrl": token_url,
                "clientId": client_id,
                "clientSecret": client_secret,
                "refresh_token": refresh_token,
            });
            let out = handler.execute("oauth2.refresh_token", _state_name, &in_json)?;
            // update connection
            if let Some(at) = out.get("access_token").and_then(|v| v.as_str()) {
                conn.update_access_token(at.to_string());
            }
            if let Some(rt) = out.get("refresh_token").and_then(|v| v.as_str()) {
                conn.update_refresh_token(Some(rt.to_string()));
            }
            if let Some(ex) = out.get("expires_in").and_then(|v| v.as_i64()) {
                conn.expires_at = Some(now + Duration::seconds(ex));
            }
            futures::executor::block_on(self.store.put(cref, &conn))?;
        }

        Ok(serde_json::to_value(conn)?)
    }
}
