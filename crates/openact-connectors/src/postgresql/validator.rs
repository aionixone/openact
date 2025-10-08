use crate::error::ConnectorError;
use serde_json::Value as JsonValue;

pub fn validate_postgres_connection(config: &JsonValue) -> Result<(), ConnectorError> {
    let obj = config.as_object().ok_or_else(|| {
        ConnectorError::InvalidConfig("Postgres connection config must be an object".into())
    })?;

    require_string(obj, "host")?;
    require_integer(obj, "port")?;
    require_string(obj, "database")?;
    require_string(obj, "user")?;

    Ok(())
}

pub fn validate_postgres_action(config: &JsonValue) -> Result<(), ConnectorError> {
    let obj = config.as_object().ok_or_else(|| {
        ConnectorError::InvalidConfig("Postgres action config must be an object".into())
    })?;

    require_string(obj, "statement")?;
    Ok(())
}

fn require_string(
    map: &serde_json::Map<String, JsonValue>,
    field: &str,
) -> Result<String, ConnectorError> {
    match map.get(field) {
        Some(JsonValue::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => Err(ConnectorError::InvalidConfig(format!(
            "Field '{}' must be a non-empty string",
            field
        ))),
        None => Err(ConnectorError::InvalidConfig(format!("Missing required field '{}'", field))),
    }
}

fn require_integer(
    map: &serde_json::Map<String, JsonValue>,
    field: &str,
) -> Result<i64, ConnectorError> {
    match map.get(field) {
        Some(JsonValue::Number(num)) if num.as_i64().is_some() => Ok(num.as_i64().unwrap()),
        Some(_) => {
            Err(ConnectorError::InvalidConfig(format!("Field '{}' must be a valid integer", field)))
        }
        None => Err(ConnectorError::InvalidConfig(format!("Missing required field '{}'", field))),
    }
}
