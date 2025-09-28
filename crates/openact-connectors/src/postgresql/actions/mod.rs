use crate::{ConnectorError, ConnectorResult};
use serde::Deserialize;
use serde_json::Value;

/// Runtime representation of a PostgreSQL action.
#[derive(Debug, Clone)]
pub struct PostgresAction {
    pub statement: String,
    pub parameters: Vec<ActionParameter>,
}

impl PostgresAction {
    /// Build an action configuration from raw JSON (stored in the ActionRecord).
    pub fn from_json(value: Value) -> ConnectorResult<Self> {
        let raw: RawPostgresAction = serde_json::from_value(value).map_err(|err| {
            ConnectorError::InvalidConfig(format!("Invalid PostgreSQL action config: {}", err))
        })?;

        let statement = raw.statement.clone().ok_or_else(|| {
            ConnectorError::InvalidConfig("Missing 'statement' field".to_string())
        })?;

        let parameters = extract_parameters(&raw)
            .into_iter()
            .map(ActionParameter::from)
            .collect();

        Ok(Self {
            statement,
            parameters,
        })
    }

    /// Returns the ordered parameter names for this action.
    pub fn parameter_names(&self) -> Vec<&str> {
        self.parameters.iter().map(|p| p.name.as_str()).collect()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawPostgresAction {
    statement: Option<String>,
    #[serde(default)]
    parameters: Option<Vec<RawActionParameter>>,
    #[serde(rename = "_metadata", default)]
    metadata: Option<ActionMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ActionMetadata {
    #[serde(default)]
    parameters: Vec<RawActionParameter>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawActionParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: Option<String>,
}

/// Parameter specification for binding input values.
#[derive(Debug, Clone)]
pub struct ActionParameter {
    pub name: String,
    pub param_type: Option<String>,
}

impl From<RawActionParameter> for ActionParameter {
    fn from(raw: RawActionParameter) -> Self {
        Self {
            name: raw.name,
            param_type: raw.param_type.map(|t| t.to_ascii_lowercase()),
        }
    }
}

fn extract_parameters(raw: &RawPostgresAction) -> Vec<RawActionParameter> {
    if let Some(params) = &raw.parameters {
        return params.clone();
    }

    raw.metadata
        .as_ref()
        .map(|meta| meta.parameters.clone())
        .unwrap_or_default()
}
