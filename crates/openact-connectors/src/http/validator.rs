use crate::error::ConnectorError;
use serde_json::Value as JsonValue;

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
