use crate::utils::error::{OpenApiToolError, Result};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::Path;
use std::str::FromStr;

/// Action Database Service
pub struct ActionDatabase {
    pub pool: SqlitePool,
}

impl ActionDatabase {
    /// Create a new database connection
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| OpenApiToolError::database(format!("Invalid database URL: {}", e)))?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await.map_err(|e| {
            OpenApiToolError::database(format!("Failed to connect to database: {}", e))
        })?;

        let db = Self { pool };
        db.initialize().await?;

        Ok(db)
    }

    /// Create a database connection from a file path
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OpenApiToolError::database(format!("Failed to create database directory: {}", e))
            })?;
        }

        let database_url = format!("sqlite:{}", path.display());
        Self::new(&database_url).await
    }

    /// Initialize the database table structure
    async fn initialize(&self) -> Result<()> {
        println!("🗄️  Initializing OpenAct database...");

        // Run migration files
        self.run_migrations().await?;

        println!("✅ OpenAct database initialized successfully");
        Ok(())
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        // Create table structure
        let create_tables_sql = include_str!("../../migrations/001_openact_tables.sql");
        sqlx::query(create_tables_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to create tables: {}", e)))?;

        // Create indexes
        let create_indexes_sql = include_str!("../../migrations/002_openact_indexes.sql");
        sqlx::query(create_indexes_sql)
            .execute(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to create indexes: {}", e)))?;

        Ok(())
    }

    /// Get the database connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Check the database connection
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                OpenApiToolError::database(format!("Database health check failed: {}", e))
            })?;

        Ok(())
    }

    /// Get database statistics
    pub async fn get_database_stats(&self) -> Result<DatabaseStats> {
        let actions_count = sqlx::query_scalar!("SELECT COUNT(*) FROM actions")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                OpenApiToolError::database(format!("Failed to get actions count: {}", e))
            })?;

        let executions_count = sqlx::query_scalar!("SELECT COUNT(*) FROM action_executions")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                OpenApiToolError::database(format!("Failed to get executions count: {}", e))
            })?;

        let tests_count = sqlx::query_scalar!("SELECT COUNT(*) FROM action_tests")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to get tests count: {}", e)))?;

        let metrics_count = sqlx::query_scalar!("SELECT COUNT(*) FROM action_metrics")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                OpenApiToolError::database(format!("Failed to get metrics count: {}", e))
            })?;

        Ok(DatabaseStats {
            total_actions: actions_count as u64,
            total_executions: executions_count as u64,
            total_tests: tests_count as u64,
            total_metrics: metrics_count as u64,
        })
    }

    /// Clean up the database
    pub async fn cleanup(&self, older_than_days: i64) -> Result<CleanupStats> {
        let cutoff_date =
            (chrono::Utc::now() - chrono::Duration::days(older_than_days)).naive_utc();

        // Clean up old execution records
        let executions_deleted = sqlx::query!(
            "DELETE FROM action_executions WHERE created_at < ?",
            cutoff_date
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to cleanup executions: {}", e)))?
        .rows_affected();

        // Clean up old test results
        let test_results_deleted = sqlx::query!(
            "DELETE FROM action_test_results WHERE created_at < ?",
            cutoff_date
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to cleanup test results: {}", e)))?
        .rows_affected();

        // Clean up old metrics data
        let metrics_deleted = sqlx::query!(
            "DELETE FROM action_metrics WHERE timestamp < ?",
            cutoff_date
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to cleanup metrics: {}", e)))?
        .rows_affected();

        Ok(CleanupStats {
            executions_deleted,
            test_results_deleted,
            metrics_deleted,
        })
    }
}

/// Database statistics information
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_actions: u64,
    pub total_executions: u64,
    pub total_tests: u64,
    pub total_metrics: u64,
}

/// Cleanup statistics information
#[derive(Debug, Clone)]
pub struct CleanupStats {
    pub executions_deleted: u64,
    pub test_results_deleted: u64,
    pub metrics_deleted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = ActionDatabase::from_file(&db_path).await.unwrap();

        // Test health check
        db.health_check().await.unwrap();

        // Test statistics
        let stats = db.get_database_stats().await.unwrap();
        assert_eq!(stats.total_actions, 0);
        assert_eq!(stats.total_executions, 0);
    }

    #[tokio::test]
    async fn test_database_cleanup() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_cleanup.db");

        let db = ActionDatabase::from_file(&db_path).await.unwrap();

        // Test cleanup (no data should be deleted)
        let cleanup_stats = db.cleanup(30).await.unwrap();
        assert_eq!(cleanup_stats.executions_deleted, 0);
        assert_eq!(cleanup_stats.test_results_deleted, 0);
        assert_eq!(cleanup_stats.metrics_deleted, 0);
    }
}
