use crate::error::ConnectorError;
use serde_json::{Value as JsonValue, Map};
use std::collections::HashSet;

/// Fields allowed in HTTP connection configuration
pub const HTTP_CONNECTION_FIELDS: &[&str] = &[
    "base_url",
    "timeout", 
    "headers",
    "authorization",
    "auth_parameters",
    "retry_policy",
    "verify_ssl",
    "proxy",
    "user_agent",
];

/// Fields allowed in HTTP action configuration
pub const HTTP_ACTION_FIELDS: &[&str] = &[
    "method",
    "path", 
    "headers",
    "query",
    "body",
    "timeout",
    "expected_status",
    "response_format",
    "follow_redirects",
    "stream_response",
];

/// Metadata fields that should never be included in config_json
const METADATA_FIELDS: &[&str] = &[
    "connection", "connector", "description", "mcp_enabled", "mcp",
    "mcp_overrides", "name", "trn", "version", "created_at", "updated_at", 
    "tenant", "kind"
];

/// Filter HTTP connection config to only include allowed fields
pub fn filter_http_connection_fields(config: &Map<String, JsonValue>) -> Map<String, JsonValue> {
    let allowed: HashSet<&str> = HTTP_CONNECTION_FIELDS.iter().copied().collect();
    config.iter()
        .filter(|(key, _)| {
            // Always exclude metadata fields
            if METADATA_FIELDS.contains(&key.as_str()) {
                return false;
            }
            // Include if in whitelist
            allowed.contains(key.as_str())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Filter HTTP action config to only include allowed fields
pub fn filter_http_action_fields(config: &Map<String, JsonValue>) -> Map<String, JsonValue> {
    let allowed: HashSet<&str> = HTTP_ACTION_FIELDS.iter().copied().collect();
    config.iter()
        .filter(|(key, _)| {
            // Always exclude metadata fields
            if METADATA_FIELDS.contains(&key.as_str()) {
                return false;
            }
            // Include if in whitelist
            allowed.contains(key.as_str())
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub fn validate_http_connection(config: &JsonValue) -> Result<(), ConnectorError> {
    let obj = config.as_object().ok_or_else(|| {
        ConnectorError::InvalidConfig("HTTP connection config must be an object".to_string())
    })?;

    // base_url must be a non-empty string starting with http/https
    match obj.get("base_url") {
        Some(JsonValue::String(url))
            if url.starts_with("http://") || url.starts_with("https://") => {}
        Some(_) => {
            return Err(ConnectorError::InvalidConfig(
                "HTTP connection base_url must be a string beginning with http:// or https://"
                    .into(),
            ))
        }
        None => {
            return Err(ConnectorError::InvalidConfig(
                "HTTP connection requires 'base_url'".into(),
            ))
        }
    }

    Ok(())
}

pub fn validate_http_action(config: &JsonValue) -> Result<(), ConnectorError> {
    let obj = config.as_object().ok_or_else(|| {
        ConnectorError::InvalidConfig("HTTP action config must be an object".to_string())
    })?;

    match obj.get("method") {
        Some(JsonValue::String(method)) => {
            let upper = method.to_uppercase();
            let allowed = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
            if !allowed.contains(&upper.as_str()) {
                return Err(ConnectorError::InvalidConfig(format!(
                    "Unsupported HTTP method: {}",
                    method
                )));
            }
        }
        _ => {
            return Err(ConnectorError::InvalidConfig(
                "HTTP action requires string 'method'".into(),
            ));
        }
    }

    match obj.get("path") {
        Some(JsonValue::String(path)) if path.starts_with('/') => Ok(()),
        Some(JsonValue::String(_)) => Err(ConnectorError::InvalidConfig(
            "HTTP action path must start with '/'".into(),
        )),
        _ => Err(ConnectorError::InvalidConfig(
            "HTTP action requires string 'path'".into(),
        )),
    }
}
