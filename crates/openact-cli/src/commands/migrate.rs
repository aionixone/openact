//! Database migration command

use crate::{error::CliResult, utils::ColoredOutput};
use openact_core::store::ConnectionStore;
use openact_store::sql_store::SqlStore;
use tracing::{info, warn};

pub struct MigrateCommand;

impl MigrateCommand {
    pub async fn run(db_path: &str, force: bool) -> CliResult<()> {
        info!("Initializing database at: {}", db_path);

        if force {
            warn!("Force flag enabled - existing database will be reset");
        }

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create database connection
        let store = SqlStore::new(db_path).await?;

        // Run migrations
        store.migrate().await?;

        println!("{}", ColoredOutput::success("✓ Database initialized successfully"));

        // Display database info
        let connectors = ConnectionStore::list_distinct_connectors(&store).await?;
        let stats = connectors.len();
        println!("Database path: {}", ColoredOutput::highlight(db_path));
        println!("Connections: {}", stats);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_migrate_new_database() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        // Should succeed for new database
        let result = MigrateCommand::run(db_path_str, false).await;
        assert!(result.is_ok());

        // Database file should exist
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_migrate_existing_database() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        // Create database first time
        MigrateCommand::run(db_path_str, false).await.unwrap();

        // Should succeed for existing database (idempotent)
        let result = MigrateCommand::run(db_path_str, false).await;
        assert!(result.is_ok());
    }
}
