use openact_core::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Core error: {0}")]
    Core(#[from] CoreError),

    #[error("Invalid TRN format: {0}")]
    InvalidTrn(String),

    #[error("Invalid connector type: {0}")]
    InvalidConnector(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),
}

// Add conversion from ConfigError to CoreError
impl From<ConfigError> for CoreError {
    fn from(err: ConfigError) -> Self {
        match err {
            ConfigError::Io(e) => CoreError::Io(e.to_string()),
            ConfigError::Yaml(e) => CoreError::Serde(e.to_string()),
            ConfigError::Json(e) => CoreError::Serde(e.to_string()),
            ConfigError::Core(e) => e,
            ConfigError::InvalidTrn(msg) => CoreError::Invalid(msg),
            ConfigError::InvalidConnector(msg) => CoreError::Invalid(msg),
            ConfigError::MissingField(msg) => CoreError::Invalid(msg),
            ConfigError::Validation(msg) => CoreError::Invalid(msg),
            ConfigError::UnsupportedFormat(msg) => CoreError::Invalid(msg),
        }
    }
}

pub type ConfigResult<T> = Result<T, ConfigError>;
