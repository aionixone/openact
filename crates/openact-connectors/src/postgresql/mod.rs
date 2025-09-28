use crate::{ConnectorError, ConnectorResult};
use actions::{ActionParameter, PostgresAction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::postgres::{PgArguments, PgPoolOptions, PgRow, PgTypeInfo};
use sqlx::{Column, Pool, Postgres, Row, TypeInfo, ValueRef};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use url::form_urlencoded::Serializer;

pub mod actions;
pub mod validator;

/// Connection configuration for PostgreSQL connectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConnection {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
}

impl PostgresConnection {
    /// Create a connection pool from this configuration.
    pub async fn create_pool(&self) -> ConnectorResult<Pool<Postgres>> {
        let connection_url = self.build_connection_url()?;
        let mut options = PgPoolOptions::new();
        options = options.max_connections(self.max_connections.unwrap_or(5));
        if let Some(timeout) = self.connect_timeout_seconds {
            options = options.acquire_timeout(Duration::from_secs(timeout));
        }

        let pool = options.connect(&connection_url).await.map_err(|err| {
            ConnectorError::Connection(format!("Failed to connect to Postgres: {}", err))
        })?;

        Ok(pool)
    }

    fn build_connection_url(&self) -> ConnectorResult<String> {
        if self.host.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "PostgreSQL host cannot be empty".to_string(),
            ));
        }
        if self.database.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "PostgreSQL database cannot be empty".to_string(),
            ));
        }

        let mut params = self.query_params.clone();
        let application_name = self
            .application_name
            .clone()
            .unwrap_or_else(|| "openact".to_string());
        params
            .entry("application_name".to_string())
            .or_insert(application_name);

        if let Some(ssl_mode) = &self.ssl_mode {
            params
                .entry("sslmode".to_string())
                .or_insert(ssl_mode.clone());
        }

        let user = urlencoding::encode(&self.user);
        let credentials = if let Some(password) = &self.password {
            format!("{}:{}", user, urlencoding::encode(password))
        } else {
            user.into_owned()
        };

        let mut connection = format!(
            "postgres://{}@{}:{}/{}",
            credentials, self.host, self.port, self.database
        );

        if !params.is_empty() {
            let mut serializer = Serializer::new(String::new());
            for (key, value) in params {
                serializer.append_pair(&key, &value);
            }
            connection.push('?');
            connection.push_str(&serializer.finish());
        }

        Ok(connection)
    }
}

/// PostgreSQL executor that owns a connection pool.
#[derive(Clone)]
pub struct PostgresExecutor {
    pool: Arc<Pool<Postgres>>,
}

impl PostgresExecutor {
    pub async fn from_connection(connection: &PostgresConnection) -> ConnectorResult<Self> {
        let pool = connection.create_pool().await?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub fn from_pool(pool: Pool<Postgres>) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    pub async fn health_check(&self) -> ConnectorResult<()> {
        self.pool.acquire().await.map(|_| ()).map_err(|err| {
            ConnectorError::Connection(format!("Postgres health check failed: {}", err))
        })
    }

    pub async fn execute(&self, action: &PostgresAction, input: Value) -> ConnectorResult<Value> {
        let args = prepare_arguments(action, input)?;
        let expects_rows = statement_returns_rows(&action.statement);

        if expects_rows {
            let rows = self.fetch_rows(&action.statement, &args, action).await?;
            Ok(Value::Array(rows))
        } else {
            let affected = self
                .execute_command(&action.statement, &args, action)
                .await?;
            Ok(json!({ "rows_affected": affected }))
        }
    }

    async fn fetch_rows(
        &self,
        statement: &str,
        args: &[Value],
        action: &PostgresAction,
    ) -> ConnectorResult<Vec<Value>> {
        let mut query = sqlx::query(statement);
        query = bind_arguments(query, args, &action.parameters)?;

        let rows = query
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|err| ConnectorError::ExecutionFailed(err.to_string()))?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(convert_row(&row)?);
        }

        Ok(results)
    }

    async fn execute_command(
        &self,
        statement: &str,
        args: &[Value],
        action: &PostgresAction,
    ) -> ConnectorResult<u64> {
        let mut query = sqlx::query(statement);
        query = bind_arguments(query, args, &action.parameters)?;

        let result = query
            .execute(self.pool.as_ref())
            .await
            .map_err(|err| ConnectorError::ExecutionFailed(err.to_string()))?;

        Ok(result.rows_affected())
    }
}

fn prepare_arguments(action: &PostgresAction, input: Value) -> ConnectorResult<Vec<Value>> {
    match input {
        Value::Null => Ok(vec![]),
        Value::Array(values) => {
            if !action.parameters.is_empty() && values.len() != action.parameters.len() {
                return Err(ConnectorError::Validation(format!(
                    "Expected {} parameters, received {}",
                    action.parameters.len(),
                    values.len()
                )));
            }
            Ok(values)
        }
        Value::Object(map) => {
            if let Some(Value::Array(values)) = map.get("args") {
                return prepare_arguments(action, Value::Array(values.clone()));
            }

            if action.parameters.is_empty() {
                return Err(ConnectorError::Validation(
                    "PostgreSQL action does not define parameters but input object provided"
                        .to_string(),
                ));
            }

            let mut ordered: Vec<Value> = Vec::with_capacity(action.parameters.len());
            for param in &action.parameters {
                match map.get(&param.name) {
                    Some(value) => ordered.push(value.clone()),
                    None => {
                        return Err(ConnectorError::Validation(format!(
                            "Missing value for parameter '{}'",
                            param.name
                        )))
                    }
                }
            }

            Ok(ordered)
        }
        other => Err(ConnectorError::Validation(format!(
            "Unsupported input payload for PostgreSQL action: {}",
            other
        ))),
    }
}

fn bind_arguments<'q>(
    mut query: sqlx::query::Query<'q, Postgres, PgArguments>,
    args: &'q [Value],
    parameters: &'q [ActionParameter],
) -> ConnectorResult<sqlx::query::Query<'q, Postgres, PgArguments>> {
    for (idx, value) in args.iter().enumerate() {
        let param_type = parameters.get(idx).and_then(|p| p.param_type.as_deref());
        query = bind_single_argument(query, value, param_type)?;
    }
    Ok(query)
}

fn bind_single_argument<'q>(
    query: sqlx::query::Query<'q, Postgres, PgArguments>,
    value: &Value,
    param_type: Option<&str>,
) -> ConnectorResult<sqlx::query::Query<'q, Postgres, PgArguments>> {
    use sqlx::types::Json;

    let mut query = query;

    match (param_type.unwrap_or(""), value) {
        ("string", Value::Null) => {
            query = query.bind::<Option<String>>(None);
        }
        ("string", Value::String(text)) => {
            query = query.bind(text.clone());
        }
        ("string", other) => {
            return Err(ConnectorError::Validation(format!(
                "Parameter expected string but received {}",
                other
            )));
        }
        ("number", Value::Null) => {
            query = query.bind::<Option<f64>>(None);
        }
        ("number", Value::Number(num)) => {
            if let Some(v) = num.as_i64() {
                query = query.bind(v);
            } else if let Some(v) = num.as_u64() {
                query = query.bind(v as i64);
            } else if let Some(v) = num.as_f64() {
                query = query.bind(v);
            } else {
                return Err(ConnectorError::Validation(
                    "Unsupported numeric value".to_string(),
                ));
            }
        }
        ("number", other) => {
            return Err(ConnectorError::Validation(format!(
                "Parameter expected number but received {}",
                other
            )));
        }
        ("boolean", Value::Null) => {
            query = query.bind::<Option<bool>>(None);
        }
        ("boolean", Value::Bool(flag)) => {
            query = query.bind(*flag);
        }
        ("boolean", other) => {
            return Err(ConnectorError::Validation(format!(
                "Parameter expected boolean but received {}",
                other
            )));
        }
        ("object", Value::Null) => {
            query = query.bind::<Option<Json<Value>>>(None);
        }
        ("object", Value::Object(_)) | ("array", Value::Array(_)) => {
            query = query.bind(Json(value.clone()));
        }
        ("array", Value::Null) => {
            query = query.bind::<Option<Json<Value>>>(None);
        }
        (other_type, other_value) if !other_type.is_empty() => {
            return Err(ConnectorError::Validation(format!(
                "Unsupported parameter type '{}' for value {}",
                other_type, other_value
            )));
        }
        (_, Value::Null) => {
            query = query.bind::<Option<String>>(None);
        }
        (_, Value::Bool(flag)) => {
            query = query.bind(*flag);
        }
        (_, Value::Number(num)) => {
            if let Some(v) = num.as_i64() {
                query = query.bind(v);
            } else if let Some(v) = num.as_u64() {
                query = query.bind(v as i64);
            } else if let Some(v) = num.as_f64() {
                query = query.bind(v);
            } else {
                return Err(ConnectorError::Validation(
                    "Unsupported numeric value".to_string(),
                ));
            }
        }
        (_, Value::String(text)) => {
            query = query.bind(text.clone());
        }
        (_, Value::Array(_)) | (_, Value::Object(_)) => {
            query = query.bind(Json(value.clone()));
        }
    }

    Ok(query)
}

fn statement_returns_rows(statement: &str) -> bool {
    let trimmed = statement.trim_start().to_lowercase();
    trimmed.starts_with("select")
        || trimmed.starts_with("with")
        || trimmed.starts_with("show")
        || trimmed.contains(" returning ")
}

fn convert_row(row: &PgRow) -> ConnectorResult<Value> {
    let mut obj = Map::with_capacity(row.len());
    for column in row.columns() {
        let idx = column.ordinal();
        let value = extract_column(row, idx, column.type_info())?;
        obj.insert(column.name().to_string(), value);
    }
    Ok(Value::Object(obj))
}

fn extract_column(row: &PgRow, idx: usize, type_info: &PgTypeInfo) -> ConnectorResult<Value> {
    let raw = row.try_get_raw(idx)?;
    if raw.is_null() {
        return Ok(Value::Null);
    }

    let type_name = type_info.name().to_ascii_uppercase();

    let value =
        match type_name.as_str() {
            "BOOL" | "BOOLEAN" => Value::Bool(row.try_get::<bool, _>(idx)?),
            "INT2" => {
                let v: i16 = row.try_get(idx)?;
                Value::Number(serde_json::Number::from(v as i64))
            }
            "INT4" => {
                let v: i32 = row.try_get(idx)?;
                Value::Number(serde_json::Number::from(v as i64))
            }
            "INT8" => {
                let v: i64 = row.try_get(idx)?;
                Value::Number(serde_json::Number::from(v))
            }
            "FLOAT4" => {
                let v: f32 = row.try_get(idx)?;
                Value::Number(serde_json::Number::from_f64(v as f64).ok_or_else(|| {
                    ConnectorError::ExecutionFailed("Invalid f32 value".to_string())
                })?)
            }
            "FLOAT8" => {
                let v: f64 = row.try_get(idx)?;
                Value::Number(serde_json::Number::from_f64(v).ok_or_else(|| {
                    ConnectorError::ExecutionFailed("Invalid f64 value".to_string())
                })?)
            }
            "NUMERIC" | "DECIMAL" => Value::String(row.try_get::<String, _>(idx)?),
            "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "UUID" => {
                Value::String(row.try_get::<String, _>(idx)?)
            }
            "TIMESTAMP" => {
                let ts = row.try_get::<chrono::NaiveDateTime, _>(idx)?;
                Value::String(ts.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
            }
            "TIMESTAMPTZ" => {
                let ts = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx)?;
                Value::String(ts.to_rfc3339())
            }
            "DATE" => {
                let date = row.try_get::<chrono::NaiveDate, _>(idx)?;
                Value::String(date.to_string())
            }
            "TIME" => {
                let time = row.try_get::<chrono::NaiveTime, _>(idx)?;
                Value::String(time.format("%H:%M:%S%.f").to_string())
            }
            "JSON" | "JSONB" => row.try_get::<Value, _>(idx)?,
            "BYTEA" => {
                let bytes: Vec<u8> = row.try_get(idx)?;
                Value::String(bytes_to_hex(&bytes))
            }
            _ => match row.try_get::<String, _>(idx) {
                Ok(text) => Value::String(text),
                Err(_) => {
                    let bytes: Vec<u8> = row.try_get(idx)?;
                    Value::String(bytes_to_hex(&bytes))
                }
            },
        };

    Ok(value)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut out, "{:02x}", byte).expect("write to string");
    }
    format!("\\x{}", out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_connection_url() {
        let mut conn = PostgresConnection {
            host: "localhost".to_string(),
            port: 5432,
            database: "demo".to_string(),
            user: "postgres".to_string(),
            password: Some("s3cret".to_string()),
            query_params: HashMap::new(),
            application_name: None,
            ssl_mode: None,
            connect_timeout_seconds: None,
            max_connections: None,
        };

        conn.query_params
            .insert("search_path".to_string(), "public".to_string());

        let url = conn.build_connection_url().unwrap();
        assert!(url.contains("postgres://postgres:s3cret@localhost:5432/demo"));
        assert!(url.contains("search_path=public"));
        assert!(url.contains("application_name=openact"));
    }
}
