use crate::error::{CoreError, Result};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

#[derive(Clone)]
pub struct CoreDatabase {
    pool: SqlitePool,
}

impl CoreDatabase {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(database_url)
            .map_err(CoreError::Database)?
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool { &self.pool }

    pub async fn migrate_bindings(&self) -> Result<()> {
        // Create bindings table and indexes only
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bindings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tenant TEXT NOT NULL,
                auth_trn TEXT NOT NULL,
                action_trn TEXT NOT NULL,
                created_by TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(tenant, auth_trn, action_trn)
            )
            "#,
        )
        .execute(self.pool())
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bindings_tenant ON bindings(tenant)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bindings_auth_trn ON bindings(auth_trn)",
        )
        .execute(self.pool())
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bindings_action_trn ON bindings(action_trn)",
        )
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Simple health check
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(self.pool()).await?;
        Ok(())
    }

    async fn count_table(&self, table: &str) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) as cnt FROM {}", table);
        match sqlx::query_scalar::<_, i64>(&sql).fetch_one(self.pool()).await {
            Ok(v) => Ok(v as u64),
            Err(_) => Ok(0),
        }
    }

    /// Basic stats across core tables
    pub async fn stats(&self) -> Result<CoreStats> {
        let bindings = self.count_table("bindings").await?;
        let actions = self.count_table("actions").await?;
        let auth_connections = self.count_table("auth_connections").await?;
        Ok(CoreStats { bindings, actions, auth_connections })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreStats {
    pub bindings: u64,
    pub actions: u64,
    pub auth_connections: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_and_stats_on_fresh_memory_db() {
        let db = CoreDatabase::connect("sqlite::memory:").await.unwrap();
        db.migrate_bindings().await.unwrap();
        db.health_check().await.unwrap();
        let s = db.stats().await.unwrap();
        assert_eq!(s.bindings, 0);
        // actions/auth_connections tables do not exist in memory setup → treated as 0
        assert_eq!(s.actions, 0);
        assert_eq!(s.auth_connections, 0);
    }
}
