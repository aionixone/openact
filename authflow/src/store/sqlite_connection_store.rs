//! SQLite connection store implementation
//! 
//! Provides a persistent connection store based on SQLite, supporting encryption and audit logs

#[cfg(feature = "sqlite")]
use sqlx::{SqlitePool, Row, sqlite::SqliteRow};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::time::Duration;

use crate::store::{
    ConnectionStore, Connection, AuthConnectionTrn,
    encryption::{FieldEncryption, EncryptedField},
};

/// SQLite storage configuration
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    /// Database file path
    pub database_url: String,
    /// Maximum number of connections
    pub max_connections: u32,
    /// Enable audit log
    pub enable_audit_log: bool,
    /// Automatically clean up expired data
    pub auto_cleanup_expired: bool,
    /// Cleanup interval
    pub cleanup_interval: Duration,
    /// Enable encryption
    pub enable_encryption: bool,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite:./data/authflow.db".to_string(),
            max_connections: 10,
            enable_audit_log: true,
            auto_cleanup_expired: true,
            cleanup_interval: Duration::from_secs(3600), // 1 hour
            enable_encryption: true,
        }
    }
}

/// SQLite connection store implementation
#[cfg(feature = "sqlite")]
pub struct SqliteConnectionStore {
    pool: SqlitePool,
    config: SqliteConfig,
    encryption: Option<FieldEncryption>,
}

#[cfg(feature = "sqlite")]
impl SqliteConnectionStore {
    /// Create a new SQLite connection store
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        // Ensure the database file exists (if it's a file database)
        if config.database_url.starts_with("sqlite:") && !config.database_url.contains(":memory:") {
            let db_path = config.database_url.strip_prefix("sqlite:").unwrap_or(&config.database_url);
            if let Some(parent) = std::path::Path::new(db_path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("Failed to create database directory: {}", e))?;
            }
            // Create an empty file (if it doesn't exist)
            if !std::path::Path::new(db_path).exists() {
                std::fs::File::create(db_path)
                    .map_err(|e| anyhow!("Failed to create database file: {}", e))?;
            }
        }

        // Create connection pool
        let pool = SqlitePool::connect(&config.database_url).await
            .map_err(|e| anyhow!("Failed to connect to SQLite database: {}", e))?;

        // Initialize encryption service
        let encryption = if config.enable_encryption {
            Some(FieldEncryption::from_env()
                .unwrap_or_else(|_| {
                    eprintln!("Warning: Failed to load encryption config from env, using default");
                    FieldEncryption::new(Default::default())
                }))
        } else {
            None
        };

        let store = Self {
            pool,
            config,
            encryption,
        };

        // Initialize database tables
        store.initialize_database().await?;

        Ok(store)
    }

    /// Initialize database table structure
    async fn initialize_database(&self) -> Result<()> {
        println!("[sqlite] initializing database at {}", self.config.database_url);
        // Create connections table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS connections (
                trn TEXT PRIMARY KEY,
                tenant TEXT NOT NULL,
                provider TEXT NOT NULL,
                user_id TEXT NOT NULL,
                access_token_encrypted TEXT NOT NULL,
                access_token_nonce TEXT NOT NULL,
                refresh_token_encrypted TEXT,
                refresh_token_nonce TEXT,
                expires_at DATETIME,
                token_type TEXT DEFAULT 'Bearer',
                scope TEXT,
                extra_data_encrypted TEXT,
                extra_data_nonce TEXT,
                key_version INTEGER DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                version INTEGER DEFAULT 1
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| anyhow!("Failed to create connections table: {}", e))?;

        // Create indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_connections_tenant_provider ON connections(tenant, provider)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_connections_expires_at ON connections(expires_at)")
            .execute(&self.pool)
            .await?;

        // Create audit log table (if enabled)
        if self.config.enable_audit_log {
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS connection_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    trn TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    old_data_encrypted TEXT,
                    old_data_nonce TEXT,
                    new_data_encrypted TEXT,
                    new_data_nonce TEXT,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    reason TEXT,
                    FOREIGN KEY (trn) REFERENCES connections(trn)
                )
                "#,
            )
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to create connection_history table: {}", e))?;
        }

        println!("[sqlite] database initialized (tables ready)");
        Ok(())
    }

    /// Encrypt sensitive fields
    fn encrypt_field(&self, data: &str) -> Result<(String, String)> {
        if let Some(ref encryption) = self.encryption {
            let encrypted = encryption.encrypt_field(data)?;
            Ok((encrypted.data, encrypted.nonce))
        } else {
            // Store directly without encryption (for development only)
            Ok((base64::encode(data), "no-encryption".to_string()))
        }
    }

    /// Decrypt sensitive fields
    fn decrypt_field(&self, data: &str, nonce: &str) -> Result<String> {
        if let Some(ref encryption) = self.encryption {
            let encrypted = EncryptedField {
                data: data.to_string(),
                nonce: nonce.to_string(),
                key_version: 1, // TODO: Read from database
            };
            encryption.decrypt_field(&encrypted)
        } else {
            // Decode directly without encryption
            let decoded = base64::decode(data)
                .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
            String::from_utf8(decoded)
                .map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
        }
    }

    /// Build connection object from database row
    fn row_to_connection(&self, row: &SqliteRow) -> Result<Connection> {
        // Decrypt access token
        let access_token_encrypted: String = row.get("access_token_encrypted");
        let access_token_nonce: String = row.get("access_token_nonce");
        let access_token = self.decrypt_field(&access_token_encrypted, &access_token_nonce)?;

        // Decrypt refresh token (if exists)
        let refresh_token = if let (Ok(encrypted), Ok(nonce)) = (
            row.try_get::<String, _>("refresh_token_encrypted"),
            row.try_get::<String, _>("refresh_token_nonce")
        ) {
            Some(self.decrypt_field(&encrypted, &nonce)?)
        } else {
            None
        };

        // Decrypt extra data (if exists)
        let extra = if let (Ok(encrypted), Ok(nonce)) = (
            row.try_get::<String, _>("extra_data_encrypted"),
            row.try_get::<String, _>("extra_data_nonce")
        ) {
            let decrypted = self.decrypt_field(&encrypted, &nonce)?;
            serde_json::from_str(&decrypted).unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        // Build TRN
        let tenant: String = row.get("tenant");
        let provider: String = row.get("provider");
        let user_id: String = row.get("user_id");
        let trn = AuthConnectionTrn::new(tenant, provider, user_id)?;

        Ok(Connection {
            trn,
            access_token,
            refresh_token,
            expires_at: row.get("expires_at"),
            token_type: row.get("token_type"),
            scope: row.get("scope"),
            extra,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Record audit log
    async fn log_audit(&self, trn: &str, operation: &str, old_data: Option<&Connection>, new_data: Option<&Connection>, reason: Option<&str>) -> Result<()> {
        if !self.config.enable_audit_log {
            return Ok(());
        }

        let (old_encrypted, old_nonce) = if let Some(data) = old_data {
            let json = serde_json::to_string(data)?;
            let (encrypted, nonce) = self.encrypt_field(&json)?;
            (Some(encrypted), Some(nonce))
        } else {
            (None, None)
        };

        let (new_encrypted, new_nonce) = if let Some(data) = new_data {
            let json = serde_json::to_string(data)?;
            let (encrypted, nonce) = self.encrypt_field(&json)?;
            (Some(encrypted), Some(nonce))
        } else {
            (None, None)
        };

        sqlx::query(
            r#"
            INSERT INTO connection_history 
            (trn, operation, old_data_encrypted, old_data_nonce, new_data_encrypted, new_data_nonce, reason)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(trn)
        .bind(operation)
        .bind(old_encrypted)
        .bind(old_nonce)
        .bind(new_encrypted)
        .bind(new_nonce)
        .bind(reason)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List connections by tenant (helper method, not part of trait)
    pub async fn list_by_tenant(&self, tenant: &str) -> Result<Vec<Connection>> {
        let rows = sqlx::query("SELECT * FROM connections WHERE tenant = ? ORDER BY created_at DESC")
            .bind(tenant)
            .fetch_all(&self.pool)
            .await?;

        let mut connections = Vec::new();
        for row in rows {
            connections.push(self.row_to_connection(&row)?);
        }

        Ok(connections)
    }

    /// List connections by provider (helper method, not part of trait)
    pub async fn list_by_provider(&self, tenant: &str, provider: &str) -> Result<Vec<Connection>> {
        let rows = sqlx::query("SELECT * FROM connections WHERE tenant = ? AND provider = ? ORDER BY created_at DESC")
            .bind(tenant)
            .bind(provider)
            .fetch_all(&self.pool)
            .await?;

        let mut connections = Vec::new();
        for row in rows {
            connections.push(self.row_to_connection(&row)?);
        }

        Ok(connections)
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl ConnectionStore for SqliteConnectionStore {
    async fn get(&self, connection_ref: &str) -> Result<Option<Connection>> {
        println!("[sqlite] get connection_ref={}", connection_ref);
        let row = sqlx::query("SELECT * FROM connections WHERE trn = ?")
            .bind(connection_ref)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            Ok(Some(self.row_to_connection(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, connection_ref: &str, connection: &Connection) -> Result<()> {
        println!(
            "[sqlite] put connection_ref={} tenant={} provider={} user_id={}",
            connection_ref, connection.trn.tenant, connection.trn.provider, connection.trn.user_id
        );
        // Encrypt sensitive data
        let (access_token_encrypted, access_token_nonce) = self.encrypt_field(&connection.access_token)?;
        
        let (refresh_token_encrypted, refresh_token_nonce) = if let Some(ref token) = connection.refresh_token {
            let (encrypted, nonce) = self.encrypt_field(token)?;
            (Some(encrypted), Some(nonce))
        } else {
            (None, None)
        };

        let (extra_data_encrypted, extra_data_nonce) = if connection.extra != Value::Null {
            let json = serde_json::to_string(&connection.extra)?;
            let (encrypted, nonce) = self.encrypt_field(&json)?;
            (Some(encrypted), Some(nonce))
        } else {
            (None, None)
        };

        // Check if it already exists
        let existing = self.get(connection_ref).await?;

        if existing.is_some() {
            // Update existing record
            sqlx::query(
                r#"
                UPDATE connections SET
                    access_token_encrypted = ?, access_token_nonce = ?,
                    refresh_token_encrypted = ?, refresh_token_nonce = ?,
                    expires_at = ?, token_type = ?, scope = ?,
                    extra_data_encrypted = ?, extra_data_nonce = ?,
                    updated_at = CURRENT_TIMESTAMP,
                    version = version + 1
                WHERE trn = ?
                "#,
            )
            .bind(&access_token_encrypted)
            .bind(&access_token_nonce)
            .bind(&refresh_token_encrypted)
            .bind(&refresh_token_nonce)
            .bind(&connection.expires_at)
            .bind(&connection.token_type)
            .bind(&connection.scope)
            .bind(&extra_data_encrypted)
            .bind(&extra_data_nonce)
            .bind(connection_ref)
            .execute(&self.pool)
            .await?;

            println!("[sqlite] updated existing connection trn={}", connection_ref);
            // Record audit log
            self.log_audit(connection_ref, "update", existing.as_ref(), Some(connection), Some("Connection updated")).await?;
        } else {
            // Insert new record
            sqlx::query(
                r#"
                INSERT INTO connections 
                (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce,
                 refresh_token_encrypted, refresh_token_nonce, expires_at, token_type, scope,
                 extra_data_encrypted, extra_data_nonce, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(connection_ref)
            .bind(&connection.trn.tenant)
            .bind(&connection.trn.provider)
            .bind(&connection.trn.user_id)
            .bind(&access_token_encrypted)
            .bind(&access_token_nonce)
            .bind(&refresh_token_encrypted)
            .bind(&refresh_token_nonce)
            .bind(&connection.expires_at)
            .bind(&connection.token_type)
            .bind(&connection.scope)
            .bind(&extra_data_encrypted)
            .bind(&extra_data_nonce)
            .execute(&self.pool)
            .await?;

            println!("[sqlite] inserted new connection trn={}", connection_ref);
            // Record audit log
            self.log_audit(connection_ref, "create", None, Some(connection), Some("Connection created")).await?;
        }

        Ok(())
    }

    async fn delete(&self, connection_ref: &str) -> Result<bool> {
        // Get existing data for audit
        let existing = self.get(connection_ref).await?;

        let result = sqlx::query("DELETE FROM connections WHERE trn = ?")
            .bind(connection_ref)
            .execute(&self.pool)
            .await?;

        let deleted = result.rows_affected() > 0;

        if deleted {
            // Record audit log
            self.log_audit(connection_ref, "delete", existing.as_ref(), None, Some("Connection deleted")).await?;
        }

        Ok(deleted)
    }

    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&Connection>,
        new_value: Option<&Connection>,
    ) -> Result<bool> {
        // Begin transaction
        let mut tx = self.pool.begin().await?;

        // Get current value
        let current = sqlx::query("SELECT * FROM connections WHERE trn = ?")
            .bind(connection_ref)
            .fetch_optional(&mut *tx)
            .await?;

        let current_connection = if let Some(row) = current {
            Some(self.row_to_connection(&row)?)
        } else {
            None
        };

        // Check if it matches the expected value
        let matches = match (expected, &current_connection) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };

        if !matches {
            tx.rollback().await?;
            return Ok(false);
        }

        // Perform update
        match new_value {
            Some(new_conn) => {
                // Update or insert
                if current_connection.is_some() {
                    // Update existing record (simplified here, should encrypt in practice)
                    sqlx::query("UPDATE connections SET updated_at = CURRENT_TIMESTAMP, version = version + 1 WHERE trn = ?")
                        .bind(connection_ref)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    // Insert new record (simplified here, should encrypt in practice)
                    sqlx::query("INSERT INTO connections (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
                        .bind(connection_ref)
                        .bind(&new_conn.trn.tenant)
                        .bind(&new_conn.trn.provider)
                        .bind(&new_conn.trn.user_id)
                        .bind("encrypted_placeholder")
                        .bind("nonce_placeholder")
                        .execute(&mut *tx)
                        .await?;
                }
            }
            None => {
                // Delete
                sqlx::query("DELETE FROM connections WHERE trn = ?")
                    .bind(connection_ref)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;
        Ok(true)
    }

    async fn list_refs(&self) -> Result<Vec<String>> {
        let refs = sqlx::query_scalar::<_, String>("SELECT trn FROM connections ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(refs)
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM connections WHERE expires_at IS NOT NULL AND expires_at < CURRENT_TIMESTAMP")
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as usize)
    }

    async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections")
            .fetch_one(&self.pool)
            .await?;
        Ok(count as usize)
    }
}

// Placeholder when sqlite feature is not enabled
#[cfg(not(feature = "sqlite"))]
pub struct SqliteConnectionStore;

#[cfg(not(feature = "sqlite"))]
impl SqliteConnectionStore {
    pub async fn new(_config: SqliteConfig) -> Result<Self> {
        Err(anyhow!("SQLite feature is not enabled. Please compile with --features sqlite"))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn create_test_store() -> SqliteConnectionStore {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let config = SqliteConfig {
            database_url: format!("sqlite:{}", db_path.display()),
            enable_encryption: false, // Disable encryption for testing
            ..Default::default()
        };

        SqliteConnectionStore::new(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_sqlite_connection_store() {
        let store = create_test_store().await;
        
        // Create test connection
        let connection = Connection::new("test_tenant", "github", "user123", "access_token_123").unwrap();
        let trn = connection.connection_id();

        // Test storage
        store.put(&trn, &connection).await.unwrap();

        // Test retrieval
        let retrieved = store.get(&trn).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.access_token, "access_token_123");
        assert_eq!(retrieved.trn.provider, "github");

        // Test deletion
        let deleted = store.delete(&trn).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let retrieved = store.get(&trn).await.unwrap();
        assert!(retrieved.is_none());
    }
}
