#[allow(unused)]
use crate::encryption::Crypto;
use crate::error::{StoreError, StoreResult};
use crate::sql_store::migrations::MigrationRunner;
use async_trait::async_trait;
use openact_core::{
    store::{ActionRepository, AuthConnectionStore, ConnectionStore, RunStore},
    ActionRecord, AuthConnection, Checkpoint, ConnectionRecord, CoreResult, Trn,
};
use serde_json::Value as JsonValue;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;

/// SQLite-based store implementation
#[derive(Debug, Clone)]
pub struct SqlStore {
    pool: SqlitePool,
}

impl SqlStore {
    /// Create a new SqlStore with database URL and optional pool configuration
    pub async fn new(database_url: &str) -> StoreResult<Self> {
        Self::new_with_config(database_url, None).await
    }

    /// Create SqlStore with custom pool configuration
    pub async fn new_with_config(
        database_url: &str,
        max_connections: Option<u32>,
    ) -> StoreResult<Self> {
        let max_conn = max_connections.unwrap_or_else(|| {
            std::env::var("OPENACT_DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10)
        });

        // Robust handling for sqlite file URLs; enable create_if_missing
        let pool = if let Some(path_str) = database_url.strip_prefix("sqlite://") {
            let path = PathBuf::from(path_str);
            let options = SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true);
            SqlitePoolOptions::new()
                .max_connections(max_conn)
                .connect_with(options)
                .await?
        } else {
            // Fallback for other forms (e.g., sqlite::memory:)
            let mut options = SqliteConnectOptions::from_str(database_url)?;
            // Try to create if missing when a filename is present
            options = options.create_if_missing(true);
            SqlitePoolOptions::new()
                .max_connections(max_conn)
                .connect_with(options)
                .await?
        };

        // Configure SQLite for better performance and consistency
        sqlx::query("PRAGMA foreign_keys = ON;")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA journal_mode = WAL;")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA synchronous = NORMAL;")
            .execute(&pool)
            .await?;

        let store = Self { pool };

        // Run migrations
        let migration_runner = MigrationRunner::new(store.pool.clone());
        migration_runner.migrate().await?;

        Ok(store)
    }

    /// Create SqlStore from existing pool (for testing)
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run migrations manually
    pub async fn migrate(&self) -> StoreResult<()> {
        let migration_runner = MigrationRunner::new(self.pool.clone());
        migration_runner.migrate().await
    }
}

#[async_trait]
impl ConnectionStore for SqlStore {
    async fn upsert(&self, record: &ConnectionRecord) -> CoreResult<()> {
        let config_json =
            serde_json::to_string(&record.config_json).map_err(StoreError::Serialization)?;

        // Try update by TRN first
        let result = sqlx::query(
            r#"
            UPDATE connections
            SET connector = ?, name = ?, config_json = ?, updated_at = ?, version = ?
            WHERE trn = ?
            "#,
        )
        .bind(&record.connector.as_str())
        .bind(&record.name)
        .bind(&config_json)
        .bind(&record.updated_at)
        .bind(&record.version)
        .bind(&record.trn.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if result.rows_affected() == 0 {
            // Insert new row; will error if (connector, name) violates UNIQUE
            sqlx::query(
                r#"
                INSERT INTO connections (trn, connector, name, config_json, created_at, updated_at, version)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&record.trn.as_str())
            .bind(&record.connector.as_str())
            .bind(&record.name)
            .bind(&config_json)
            .bind(&record.created_at)
            .bind(&record.updated_at)
            .bind(&record.version)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        }

        Ok(())
    }

    async fn get(&self, trn: &Trn) -> CoreResult<Option<ConnectionRecord>> {
        let row = sqlx::query(
            "SELECT trn, connector, name, config_json, created_at, updated_at, version FROM connections WHERE trn = ?"
        )
        .bind(trn.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if let Some(row) = row {
            let config_json_str: String = row.get("config_json");
            let config_json: JsonValue =
                serde_json::from_str(&config_json_str).map_err(StoreError::Serialization)?;

            Ok(Some(ConnectionRecord {
                trn: Trn::new(row.get::<String, _>("trn")),
                connector: openact_core::ConnectorKind::new(row.get::<String, _>("connector")),
                name: row.get("name"),
                config_json,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, trn: &Trn) -> CoreResult<bool> {
        let result = sqlx::query("DELETE FROM connections WHERE trn = ?")
            .bind(trn.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_by_connector(&self, connector: &str) -> CoreResult<Vec<ConnectionRecord>> {
        let rows = sqlx::query(
            "SELECT trn, connector, name, config_json, created_at, updated_at, version FROM connections WHERE connector = ? ORDER BY created_at"
        )
        .bind(connector)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let mut records = Vec::new();
        for row in rows {
            let config_json_str: String = row.get("config_json");
            let config_json: JsonValue =
                serde_json::from_str(&config_json_str).map_err(StoreError::Serialization)?;

            records.push(ConnectionRecord {
                trn: Trn::new(row.get::<String, _>("trn")),
                connector: openact_core::ConnectorKind::new(row.get::<String, _>("connector")),
                name: row.get("name"),
                config_json,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            });
        }

        Ok(records)
    }

    async fn list_distinct_connectors(&self) -> CoreResult<Vec<openact_core::ConnectorKind>> {
        let rows = sqlx::query("SELECT DISTINCT connector FROM connections")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        let connectors = rows
            .into_iter()
            .map(|row| openact_core::ConnectorKind::new(row.get::<String, _>("connector")))
            .collect();

        Ok(connectors)
    }
}

#[async_trait]
impl ActionRepository for SqlStore {
    async fn upsert(&self, record: &ActionRecord) -> CoreResult<()> {
        let config_json =
            serde_json::to_string(&record.config_json).map_err(StoreError::Serialization)?;

        let mcp_overrides_json = if let Some(ref overrides) = record.mcp_overrides {
            Some(serde_json::to_string(overrides).map_err(StoreError::Serialization)?)
        } else {
            None
        };

        // Try update by TRN first
        let result = sqlx::query(
            r#"
            UPDATE actions
            SET connector = ?, name = ?, connection_trn = ?, config_json = ?, mcp_enabled = ?, mcp_overrides_json = ?, updated_at = ?, version = ?
            WHERE trn = ?
            "#,
        )
        .bind(&record.connector.as_str())
        .bind(&record.name)
        .bind(&record.connection_trn.as_str())
        .bind(&config_json)
        .bind(&record.mcp_enabled)
        .bind(&mcp_overrides_json)
        .bind(&record.updated_at)
        .bind(&record.version)
        .bind(&record.trn.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if result.rows_affected() == 0 {
            // Insert new row; will error if (connection_trn, name) violates UNIQUE or FK fails
            sqlx::query(
                r#"
                INSERT INTO actions (trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&record.trn.as_str())
            .bind(&record.connector.as_str())
            .bind(&record.name)
            .bind(&record.connection_trn.as_str())
            .bind(&config_json)
            .bind(&record.mcp_enabled)
            .bind(&mcp_overrides_json)
            .bind(&record.created_at)
            .bind(&record.updated_at)
            .bind(&record.version)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        }

        Ok(())
    }

    async fn get(&self, trn: &Trn) -> CoreResult<Option<ActionRecord>> {
        let row = sqlx::query(
            "SELECT trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version FROM actions WHERE trn = ?"
        )
        .bind(trn.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if let Some(row) = row {
            let config_json_str: String = row.get("config_json");
            let config_json: JsonValue =
                serde_json::from_str(&config_json_str).map_err(StoreError::Serialization)?;

            let mcp_overrides_json: Option<String> = row.get("mcp_overrides_json");
            let mcp_overrides = if let Some(json_str) = mcp_overrides_json {
                Some(serde_json::from_str(&json_str).map_err(StoreError::Serialization)?)
            } else {
                None
            };

            Ok(Some(ActionRecord {
                trn: Trn::new(row.get::<String, _>("trn")),
                connector: openact_core::ConnectorKind::new(row.get::<String, _>("connector")),
                name: row.get("name"),
                connection_trn: Trn::new(row.get::<String, _>("connection_trn")),
                config_json,
                mcp_enabled: row.get("mcp_enabled"),
                mcp_overrides,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, trn: &Trn) -> CoreResult<bool> {
        let result = sqlx::query("DELETE FROM actions WHERE trn = ?")
            .bind(trn.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_by_connection(&self, connection_trn: &Trn) -> CoreResult<Vec<ActionRecord>> {
        let rows = sqlx::query(
            "SELECT trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version FROM actions WHERE connection_trn = ? ORDER BY created_at"
        )
        .bind(connection_trn.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let mut records = Vec::new();
        for row in rows {
            let config_json_str: String = row.get("config_json");
            let config_json: JsonValue =
                serde_json::from_str(&config_json_str).map_err(StoreError::Serialization)?;

            let mcp_overrides_json: Option<String> = row.get("mcp_overrides_json");
            let mcp_overrides = if let Some(json_str) = mcp_overrides_json {
                Some(serde_json::from_str(&json_str).map_err(StoreError::Serialization)?)
            } else {
                None
            };

            records.push(ActionRecord {
                trn: Trn::new(row.get::<String, _>("trn")),
                connector: openact_core::ConnectorKind::new(row.get::<String, _>("connector")),
                name: row.get("name"),
                connection_trn: Trn::new(row.get::<String, _>("connection_trn")),
                config_json,
                mcp_enabled: row.get("mcp_enabled"),
                mcp_overrides,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            });
        }

        Ok(records)
    }

    async fn list_by_connector(
        &self,
        connector: &openact_core::ConnectorKind,
    ) -> CoreResult<Vec<ActionRecord>> {
        let rows = sqlx::query(
            "SELECT trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version FROM actions WHERE connector = ? ORDER BY created_at"
        )
        .bind(connector.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let mut records = Vec::new();
        for row in rows {
            let config_json_str: String = row.get("config_json");
            let config_json: JsonValue =
                serde_json::from_str(&config_json_str).map_err(StoreError::Serialization)?;

            let mcp_overrides_json: Option<String> = row.get("mcp_overrides_json");
            let mcp_overrides = if let Some(json_str) = mcp_overrides_json {
                Some(serde_json::from_str(&json_str).map_err(StoreError::Serialization)?)
            } else {
                None
            };

            records.push(ActionRecord {
                trn: Trn::new(row.get::<String, _>("trn")),
                connector: openact_core::ConnectorKind::new(row.get::<String, _>("connector")),
                name: row.get("name"),
                connection_trn: Trn::new(row.get::<String, _>("connection_trn")),
                config_json,
                mcp_enabled: row.get("mcp_enabled"),
                mcp_overrides,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            });
        }

        Ok(records)
    }
}

#[async_trait]
impl RunStore for SqlStore {
    async fn put(&self, cp: Checkpoint) -> CoreResult<()> {
        let context_json =
            serde_json::to_string(&cp.context_json).map_err(StoreError::Serialization)?;
        let await_meta_json = if let Some(meta) = &cp.await_meta_json {
            Some(serde_json::to_string(meta).map_err(StoreError::Serialization)?)
        } else {
            None
        };

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO run_checkpoints (run_id, paused_state, context_json, await_meta_json, created_at, updated_at)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&cp.run_id)
        .bind(&cp.paused_state)
        .bind(&context_json)
        .bind(&await_meta_json)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn get(&self, run_id: &str) -> CoreResult<Option<Checkpoint>> {
        let row = sqlx::query(
            "SELECT run_id, paused_state, context_json, await_meta_json FROM run_checkpoints WHERE run_id = ?"
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if let Some(row) = row {
            let context_json_str: String = row.get("context_json");
            let context_json: JsonValue =
                serde_json::from_str(&context_json_str).map_err(StoreError::Serialization)?;

            let await_meta_json =
                if let Some(meta_str) = row.get::<Option<String>, _>("await_meta_json") {
                    Some(serde_json::from_str(&meta_str).map_err(StoreError::Serialization)?)
                } else {
                    None
                };

            Ok(Some(Checkpoint {
                run_id: row.get("run_id"),
                paused_state: row.get("paused_state"),
                context_json,
                await_meta_json,
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, run_id: &str) -> CoreResult<bool> {
        let result = sqlx::query("DELETE FROM run_checkpoints WHERE run_id = ?")
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl AuthConnectionStore for SqlStore {
    async fn get(&self, auth_ref: &str) -> CoreResult<Option<AuthConnection>> {
        let row = sqlx::query("SELECT * FROM auth_connections WHERE trn = ?")
            .bind(auth_ref)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        if let Some(row) = row {
            // Decrypt if encryption enabled and key present; otherwise treat as plaintext
            let extra_json: String = row.try_get("extra_data_encrypted").unwrap_or_default();
            let extra = if extra_json.is_empty() {
                JsonValue::Null
            } else {
                serde_json::from_str(&extra_json).unwrap_or(JsonValue::Null)
            };
            #[cfg(feature = "encryption")]
            let (access_token, refresh_token) = {
                let crypto = Crypto::from_env();
                if let Some(c) = crypto {
                    let ct: String = row.get("access_token_encrypted");
                    let nonce: String = row.try_get("access_token_nonce").unwrap_or_default();
                    let at = c
                        .decrypt(&ct, &nonce)
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .unwrap_or_else(|| ct);
                    let rt_ct: Option<String> = row.try_get("refresh_token_encrypted").ok();
                    let rt_nonce: String = row.try_get("refresh_token_nonce").unwrap_or_default();
                    let rt = if let Some(rt_ct) = rt_ct {
                        c.decrypt(&rt_ct, &rt_nonce)
                            .and_then(|b| String::from_utf8(b).ok())
                            .or(Some(rt_ct))
                    } else {
                        None
                    };
                    (at, rt)
                } else {
                    let at: String = row.get("access_token_encrypted");
                    let rt: Option<String> = row.try_get("refresh_token_encrypted").ok();
                    (at, rt)
                }
            };
            #[cfg(not(feature = "encryption"))]
            let access_token: String = row.get("access_token_encrypted");
            #[cfg(not(feature = "encryption"))]
            let refresh_token: Option<String> = row.try_get("refresh_token_encrypted").ok();

            Ok(Some(AuthConnection {
                trn: row.get("trn"),
                tenant: row.get("tenant"),
                provider: row.get("provider"),
                user_id: row.get("user_id"),
                access_token,
                refresh_token,
                expires_at: row.get("expires_at"),
                token_type: row.get("token_type"),
                scope: row.get("scope"),
                extra,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                version: row.get("version"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, auth_ref: &str, connection: &AuthConnection) -> CoreResult<()> {
        // Encrypt sensitive fields when encryption feature is enabled and key is present
        let extra_json =
            serde_json::to_string(&connection.extra).map_err(StoreError::Serialization)?;
        #[cfg(feature = "encryption")]
        let (
            access_token_encrypted,
            access_token_nonce,
            refresh_token_encrypted,
            refresh_token_nonce,
        ) = {
            if let Some(c) = Crypto::from_env() {
                let (ct, nonce) = c.encrypt(connection.access_token.as_bytes());
                let (rt_ct_opt, rt_nonce) = if let Some(rt) = &connection.refresh_token {
                    let (rt_ct, rt_n) = c.encrypt(rt.as_bytes());
                    (Some(rt_ct), rt_n)
                } else {
                    (None, String::new())
                };
                (ct, nonce, rt_ct_opt, rt_nonce)
            } else {
                (
                    connection.access_token.clone(),
                    String::new(),
                    connection.refresh_token.clone(),
                    String::new(),
                )
            }
        };
        #[cfg(not(feature = "encryption"))]
        let (
            access_token_encrypted,
            access_token_nonce,
            refresh_token_encrypted,
            refresh_token_nonce,
        ) = (
            connection.access_token.clone(),
            String::new(),
            connection.refresh_token.clone(),
            String::new(),
        );

        // Try to update first
        let result = sqlx::query(r#"
            UPDATE auth_connections 
            SET access_token_encrypted = ?, access_token_nonce = ?, refresh_token_encrypted = ?, refresh_token_nonce = ?, 
                expires_at = ?, token_type = ?, scope = ?, 
                extra_data_encrypted = ?, updated_at = ?, version = version + 1
            WHERE trn = ?
        "#)
        .bind(&access_token_encrypted)
        .bind(&access_token_nonce)
        .bind(&refresh_token_encrypted)
        .bind(&refresh_token_nonce)
        .bind(&connection.expires_at)
        .bind(&connection.token_type)
        .bind(&connection.scope)
        .bind(&extra_json)
        .bind(&connection.updated_at)
        .bind(auth_ref)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if result.rows_affected() == 0 {
            // Insert new record
            sqlx::query(
                r#"
                INSERT INTO auth_connections 
            (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce,
                 refresh_token_encrypted, refresh_token_nonce, expires_at, token_type, scope,
                 extra_data_encrypted, extra_data_nonce, created_at, updated_at, version)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?)
            "#,
            )
            .bind(&connection.trn)
            .bind(&connection.tenant)
            .bind(&connection.provider)
            .bind(&connection.user_id)
            .bind(&access_token_encrypted)
            .bind(&access_token_nonce)
            .bind(&refresh_token_encrypted)
            .bind(&refresh_token_nonce)
            .bind(&connection.expires_at)
            .bind(&connection.token_type)
            .bind(&connection.scope)
            .bind(&extra_json)
            .bind(&connection.created_at)
            .bind(&connection.updated_at)
            .bind(&connection.version)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;
        }

        Ok(())
    }

    async fn delete(&self, auth_ref: &str) -> CoreResult<bool> {
        let result = sqlx::query("DELETE FROM auth_connections WHERE trn = ?")
            .bind(auth_ref)
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_refs(&self) -> CoreResult<Vec<String>> {
        let rows = sqlx::query("SELECT trn FROM auth_connections ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>("trn"))
            .collect())
    }

    async fn cleanup_expired(&self) -> CoreResult<u64> {
        let result = sqlx::query("DELETE FROM auth_connections WHERE expires_at IS NOT NULL AND expires_at < datetime('now')")
            .execute(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(result.rows_affected())
    }

    async fn compare_and_swap(
        &self,
        auth_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> CoreResult<bool> {
        match (expected, new_connection) {
            (Some(exp), Some(new_conn)) => {
                // Update only if current version matches expected.version
                let extra_json =
                    serde_json::to_string(&new_conn.extra).map_err(StoreError::Serialization)?;
                #[cfg(feature = "encryption")]
                let (
                    access_token_encrypted,
                    access_token_nonce,
                    refresh_token_encrypted,
                    refresh_token_nonce,
                ) = {
                    if let Some(c) = Crypto::from_env() {
                        let (ct, nonce) = c.encrypt(new_conn.access_token.as_bytes());
                        let (rt_ct_opt, rt_nonce) = if let Some(rt) = &new_conn.refresh_token {
                            let (rt_ct, rt_n) = c.encrypt(rt.as_bytes());
                            (Some(rt_ct), rt_n)
                        } else {
                            (None, String::new())
                        };
                        (ct, nonce, rt_ct_opt, rt_nonce)
                    } else {
                        (
                            new_conn.access_token.clone(),
                            String::new(),
                            new_conn.refresh_token.clone(),
                            String::new(),
                        )
                    }
                };
                #[cfg(not(feature = "encryption"))]
                let (
                    access_token_encrypted,
                    access_token_nonce,
                    refresh_token_encrypted,
                    refresh_token_nonce,
                ) = (
                    new_conn.access_token.clone(),
                    String::new(),
                    new_conn.refresh_token.clone(),
                    String::new(),
                );

                let result = sqlx::query(
                    r#"
                    UPDATE auth_connections 
                    SET access_token_encrypted = ?,
                        access_token_nonce = ?,
                        refresh_token_encrypted = ?,
                        refresh_token_nonce = ?,
                        expires_at = ?,
                        token_type = ?,
                        scope = ?,
                        extra_data_encrypted = ?,
                        updated_at = ?,
                        version = version + 1
                    WHERE trn = ? AND version = ?
                "#,
                )
                .bind(&access_token_encrypted)
                .bind(&access_token_nonce)
                .bind(&refresh_token_encrypted)
                .bind(&refresh_token_nonce)
                .bind(&new_conn.expires_at)
                .bind(&new_conn.token_type)
                .bind(&new_conn.scope)
                .bind(&extra_json)
                .bind(&new_conn.updated_at)
                .bind(auth_ref)
                .bind(exp.version)
                .execute(&self.pool)
                .await
                .map_err(StoreError::Database)?;

                Ok(result.rows_affected() == 1)
            }
            (Some(exp), None) => {
                // Delete only if current version matches expected.version
                let result =
                    sqlx::query("DELETE FROM auth_connections WHERE trn = ? AND version = ?")
                        .bind(auth_ref)
                        .bind(exp.version)
                        .execute(&self.pool)
                        .await
                        .map_err(StoreError::Database)?;
                Ok(result.rows_affected() == 1)
            }
            (None, Some(new_conn)) => {
                // Insert only if row does not exist
                let extra_json =
                    serde_json::to_string(&new_conn.extra).map_err(StoreError::Serialization)?;

                let result = sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO auth_connections 
                    (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce,
                     refresh_token_encrypted, refresh_token_nonce, expires_at, token_type, scope,
                     extra_data_encrypted, extra_data_nonce, created_at, updated_at, version)
                    VALUES (?, ?, ?, ?, ?, '', ?, '', ?, ?, ?, ?, '', ?, ?, ?)
                "#,
                )
                .bind(&new_conn.trn)
                .bind(&new_conn.tenant)
                .bind(&new_conn.provider)
                .bind(&new_conn.user_id)
                .bind(&new_conn.access_token)
                .bind(&new_conn.refresh_token)
                .bind(&new_conn.expires_at)
                .bind(&new_conn.token_type)
                .bind(&new_conn.scope)
                .bind(&extra_json)
                .bind(&new_conn.created_at)
                .bind(&new_conn.updated_at)
                .bind(&new_conn.version)
                .execute(&self.pool)
                .await
                .map_err(StoreError::Database)?;
                Ok(result.rows_affected() == 1)
            }
            (None, None) => {
                // Succeeds only if nothing exists
                let row = sqlx::query("SELECT 1 FROM auth_connections WHERE trn = ?")
                    .bind(auth_ref)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(StoreError::Database)?;
                Ok(row.is_none())
            }
        }
    }

    async fn count(&self) -> CoreResult<u64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM auth_connections")
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Database)?;

        Ok(row.get::<i64, _>("count") as u64)
    }
}
