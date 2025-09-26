//! Database Manager
//!
//! Provides unified database connection pool management and initialization logic

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::env;
use std::path::Path;
use tokio::fs;
use tokio::sync::{Mutex as TokioMutex, OnceCell as TokioOnceCell};

use super::connection_repository::ConnectionRepository;
use crate::store::encryption::FieldEncryption;

/// Database Manager
pub struct DatabaseManager {
    pool: SqlitePool,
    encryption: Option<FieldEncryption>,
}

impl DatabaseManager {
    /// Create a database manager from environment variables
    pub async fn from_env() -> Result<Self> {
        let database_url =
            env::var("OPENACT_DB_URL").unwrap_or_else(|_| "./data/openact.db".to_string());

        Self::new(&database_url).await
    }

    /// Create a new database manager
    pub async fn new(database_url: &str) -> Result<Self> {
        // Ensure the database directory exists
        if database_url.starts_with("sqlite:") || !database_url.contains("://") {
            let db_path = if database_url.starts_with("sqlite:") {
                database_url.strip_prefix("sqlite:").unwrap_or(database_url)
            } else {
                database_url
            };

            let path = Path::new(db_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .context("Failed to create database directory")?;
            }
        }

        // Normalize the database URL to ensure write permissions
        let mut url_owned = database_url.to_string();
        // Ensure absolute paths use sqlite:// prefix (three slashes after scheme)
        if url_owned.starts_with("sqlite:/") && !url_owned.starts_with("sqlite://") {
            url_owned = url_owned.replacen("sqlite:/", "sqlite://", 1);
        }
        let normalized_url = if url_owned.starts_with("sqlite:") {
            // If it already has the sqlite prefix, check for mode parameter
            if url_owned.contains("mode=") {
                url_owned
            } else {
                let separator = if url_owned.contains("?") { "&" } else { "?" };
                format!("{}{}mode=rwc", url_owned, separator)
            }
        } else {
            // Use URL form, ensure it is a sqlite:// path to avoid permission issues on some platforms
            format!("sqlite://{}?mode=rwc", url_owned)
        };

        // Create connection pool
        let pool = SqlitePool::connect(&normalized_url)
            .await
            .context("Failed to connect to database")?;

        // Initialize encryption service
        let encryption = match env::var("OPENACT_MASTER_KEY") {
            Ok(_) => Some(FieldEncryption::from_env()?),
            Err(_) => {
                tracing::warn!(
                    "OPENACT_MASTER_KEY not set, sensitive fields will not be encrypted"
                );
                None
            }
        };

        let manager = Self { pool, encryption };

        // Initialize database schema
        manager.initialize_schema().await?;

        Ok(manager)
    }

    /// Get the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get the encryption service
    pub fn encryption(&self) -> &Option<FieldEncryption> {
        &self.encryption
    }

    /// Create a ConnectionRepository
    pub fn connection_repository(&self) -> ConnectionRepository {
        ConnectionRepository::new(self.pool.clone(), self.encryption.clone())
    }

    /// Initialize the database schema - using the migration system
    async fn initialize_schema(&self) -> Result<()> {
        // Global migration lock to avoid concurrent UNIQUE(_sqlx_migrations.version) errors in tests
        static MIGRATION_LOCK: TokioOnceCell<TokioMutex<()>> = TokioOnceCell::const_new();
        let lock = MIGRATION_LOCK
            .get_or_init(|| async { TokioMutex::new(()) })
            .await
            .lock()
            .await;
        tracing::info!("Running database migrations...");

        // Run all pending migrations
        let migration_result = sqlx::migrate!("./migrations").run(&self.pool).await;

        match migration_result {
            Ok(_) => {
                tracing::info!("Database migrations completed successfully");
                drop(lock);
                Ok(())
            }
            Err(sqlx::migrate::MigrateError::VersionMismatch(version)) => {
                let error_msg = format!(
                    "Database migration version mismatch (found: {}). This may happen when:\n\
                     1. The database was created with a different version of OpenAct\n\
                     2. Migration files have been modified\n\
                     \n\
                     To fix this, you can:\n\
                     1. Delete the database file and restart (will lose all data)\n\
                     2. Run 'openact-cli system reset-db' (if available)\n\
                     3. Backup your data and recreate the database",
                    version
                );
                drop(lock);
                Err(anyhow::anyhow!(error_msg))
            }
            Err(e) if e.to_string().contains("no such column") => {
                let error_msg = format!(
                    "Database schema is missing expected columns: {}\n\
                     This usually means the database needs to be updated.\n\
                     \n\
                     To fix this:\n\
                     1. Backup your data if needed\n\
                     2. Delete the database file (data/openact.db) to force recreation\n\
                     3. Restart the application\n\
                     \n\
                     For development: This often happens when switching between branches with different schemas.",
                    e
                );
                drop(lock);
                Err(anyhow::anyhow!(error_msg))
            }
            Err(e) => {
                drop(lock);
                Err(anyhow::anyhow!("Failed to run database migrations: {}", e))
            }
        }
    }

    /// Health check
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .context("Database health check failed")?;
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let auth_connections_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM auth_connections")
                .fetch_one(&self.pool)
                .await?;

        let connections_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections")
            .fetch_one(&self.pool)
            .await?;

        let tasks_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
            .fetch_one(&self.pool)
            .await?;

        Ok(DatabaseStats {
            auth_connections_count,
            connections_count,
            tasks_count,
        })
    }

    /// Cleanup expired authentication connections
    pub async fn cleanup_expired_auth_connections(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM auth_connections WHERE expires_at < datetime('now')")
            .execute(&self.pool)
            .await
            .context("Failed to cleanup expired auth connections")?;

        Ok(result.rows_affected())
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub auth_connections_count: i64,
    pub connections_count: i64,
    pub tasks_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_database_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database_url = db_path.to_string_lossy().to_string();

        let manager = DatabaseManager::new(&database_url).await.unwrap();

        // Health check
        manager.health_check().await.unwrap();

        // Get statistics
        let stats = manager.get_stats().await.unwrap();
        assert_eq!(stats.auth_connections_count, 0);
        assert_eq!(stats.connections_count, 0);
        assert_eq!(stats.tasks_count, 0);
    }

    #[tokio::test]
    async fn test_connection_repository_integration() {
        let database_url = "sqlite::memory:";
        let manager = DatabaseManager::new(database_url).await.unwrap();
        let _repo = manager.connection_repository();

        // Test repository creation success (verified by health check)
        manager.health_check().await.unwrap();
    }
}
