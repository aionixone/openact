use openact_core::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[cfg(feature = "sqlite")]
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Core error: {0}")]
    Core(#[from] CoreError),

    #[error("Invalid TRN format: {0}")]
    InvalidTrn(String),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// Add conversion from StoreError to CoreError
impl From<StoreError> for CoreError {
    fn from(err: StoreError) -> Self {
        match err {
            #[cfg(feature = "sqlite")]
            StoreError::Database(e) => {
                // Map specific SQLite errors to more semantic CoreError variants
                match e {
                    sqlx::Error::Database(db_err) => {
                        let code = db_err.code().unwrap_or_default();
                        let message = db_err.message();

                        // SQLite error codes: https://www.sqlite.org/rescode.html
                        match code.as_ref() {
                            "1555" | "2067" => CoreError::Conflict(format!(
                                "Unique constraint violation: {}",
                                message
                            )),
                            "787" => CoreError::Invalid(format!(
                                "Foreign key constraint failed: {}",
                                message
                            )),
                            "1032" => CoreError::Io(format!("Database is read-only: {}", message)),
                            _ => CoreError::Db(format!("Database error ({}): {}", code, message)),
                        }
                    }
                    _ => CoreError::Db(e.to_string()),
                }
            }
            StoreError::Serialization(e) => CoreError::Serde(e.to_string()),
            StoreError::Core(e) => e,
            StoreError::InvalidTrn(msg) => CoreError::Invalid(msg),
            StoreError::NotFound(msg) => CoreError::NotFound(msg),
            StoreError::Validation(msg) => CoreError::Invalid(msg),
            StoreError::Io(e) => CoreError::Io(e.to_string()),
        }
    }
}

pub type StoreResult<T> = Result<T, StoreError>;
