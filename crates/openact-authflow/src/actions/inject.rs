use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::engine::TaskHandler;

#[derive(Default)]
pub struct InjectBearerHandler;

impl TaskHandler for InjectBearerHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx: { token, headerName?: "Authorization", scheme?: "Bearer" }
        let token = ctx.get("token").and_then(|v| v.as_str()).unwrap_or("");
        let header_name = ctx
            .get("headerName")
            .and_then(|v| v.as_str())
            .unwrap_or("Authorization");
        let scheme = ctx
            .get("scheme")
            .and_then(|v| v.as_str())
            .unwrap_or("Bearer");
        let value = if token.is_empty() {
            String::from(scheme)
        } else {
            format!("{} {}", scheme, token)
        };
        Ok(json!({
            "headers": { header_name: value },
            "query": {},
            "cookies": {}
        }))
    }
}

#[derive(Default)]
pub struct InjectApiKeyHandler;

impl TaskHandler for InjectApiKeyHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx: { key, location: "header"|"query"|"cookie", name: string, prefix?: string }
        let key = ctx.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let location = ctx
            .get("location")
            .and_then(|v| v.as_str())
            .unwrap_or("header");
        let name = ctx
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("X-API-Key");
        let prefix = ctx.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
        let val = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{} {}", prefix, key)
        };
        match location {
            "header" => Ok(json!({"headers": { name: val }, "query": {}, "cookies": {}})),
            "query" => Ok(json!({"headers": {}, "query": { name: val }, "cookies": {}})),
            "cookie" | "cookies" => {
                Ok(json!({"headers": {}, "query": {}, "cookies": { name: val }}))
            }
            _ => bail!("unsupported location: {location}"),
        }
    }
}
