use crate::config::DatabaseConfig;
use crate::error::{Result, StorageError};
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};

pub type DbPool = Pool<Sqlite>;

pub async fn get_pool(cfg: &DatabaseConfig) -> Result<DbPool> {
    if cfg.dsn.starts_with("sqlite:") && !cfg.dsn.contains(":memory:") {
        let path = cfg.dsn.strip_prefix("sqlite:").unwrap_or(&cfg.dsn);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::Other(anyhow::anyhow!(e)))?;
        }
        if !std::path::Path::new(path).exists() {
            if let Err(e) = std::fs::File::create(path) {
                // It's okay if file creation fails; sqlx may create it, but ensure dir exists
                tracing::warn!("Failed to pre-create sqlite file {}: {}", path, e);
            }
        }
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(cfg.max_connections)
        .connect(&cfg.dsn)
        .await
        .map_err(StorageError::Db)?;
    Ok(pool)
}
