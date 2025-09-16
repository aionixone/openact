use anyhow::{Result, bail};
use serde_json::{Value, json};
use base64::Engine;

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

/// Basic Authentication injection handler
/// Generates Authorization header with Basic scheme
#[derive(Default)]
pub struct InjectBasicAuthHandler;

impl TaskHandler for InjectBasicAuthHandler {
    fn execute(&self, _resource: &str, _state_name: &str, ctx: &Value) -> Result<Value> {
        // ctx: { username, password, headerName?: "Authorization" }
        let username = ctx
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let password = ctx
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let header_name = ctx
            .get("headerName")
            .and_then(|v| v.as_str())
            .unwrap_or("Authorization");

        // Encode credentials as base64
        let credentials = format!("{}:{}", username, password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        let auth_value = format!("Basic {}", encoded);

        Ok(json!({
            "headers": { header_name: auth_value },
            "query": {},
            "cookies": {}
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_basic_auth_injection() {
        let handler = InjectBasicAuthHandler;
        let ctx = json!({
            "username": "user123",
            "password": "pass456"
        });

        let result = handler.execute("inject.basic", "test", &ctx).unwrap();
        
        // Verify the structure
        assert!(result.get("headers").is_some());
        let headers = result.get("headers").unwrap().as_object().unwrap();
        let auth_header = headers.get("Authorization").unwrap().as_str().unwrap();
        
        // Should start with "Basic "
        assert!(auth_header.starts_with("Basic "));
        
        // Decode and verify
        let encoded_part = &auth_header[6..]; // Remove "Basic " prefix
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded_part).unwrap();
        let credentials = String::from_utf8(decoded).unwrap();
        assert_eq!(credentials, "user123:pass456");
    }

    #[test]
    fn test_basic_auth_custom_header() {
        let handler = InjectBasicAuthHandler;
        let ctx = json!({
            "username": "admin",
            "password": "secret",
            "headerName": "X-Auth"
        });

        let result = handler.execute("inject.basic", "test", &ctx).unwrap();
        let headers = result.get("headers").unwrap().as_object().unwrap();
        
        assert!(headers.contains_key("X-Auth"));
        assert!(!headers.contains_key("Authorization"));
    }

    #[test]
    fn test_basic_auth_empty_credentials() {
        let handler = InjectBasicAuthHandler;
        let ctx = json!({});

        let result = handler.execute("inject.basic", "test", &ctx).unwrap();
        let headers = result.get("headers").unwrap().as_object().unwrap();
        let auth_header = headers.get("Authorization").unwrap().as_str().unwrap();
        
        // Should encode empty credentials as ":"
        let encoded_part = &auth_header[6..];
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded_part).unwrap();
        let credentials = String::from_utf8(decoded).unwrap();
        assert_eq!(credentials, ":");
    }
}
