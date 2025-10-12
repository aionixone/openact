#[allow(unused)]
use crate::encryption::Crypto;
use crate::error::{StoreError, StoreResult};
use crate::sql_store::migrations::MigrationRunner;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openact_core::{
    orchestration::{
        OrchestratorOutboxInsert, OrchestratorOutboxRecord, OrchestratorOutboxStore,
        OrchestratorRunRecord, OrchestratorRunStatus, OrchestratorRunStore,
    },
    store::{
        ActionListFilter, ActionListOptions, ActionListResult, ActionRepository, ActionSortField,
        AuthConnectionStore, ConnectionStore, RunStore,
    },
    ActionRecord, AuthConnection, Checkpoint, ConnectionRecord, CoreResult, Trn,
};
use serde_json::Value as JsonValue;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use std::convert::TryFrom;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use super::dedup::create_sqlite_dedup_store;
/// SQLite-based store implementation
use aionix_contracts::idempotency::DedupStore;

#[derive(Clone)]
pub struct SqlStore {
    pool: SqlitePool,
    dedup: Arc<dyn DedupStore>,
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
            let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
            SqlitePoolOptions::new().max_connections(max_conn).connect_with(options).await?
        } else {
            // Fallback for other forms (e.g., sqlite::memory:)
            let mut options = SqliteConnectOptions::from_str(database_url)?;
            // Try to create if missing when a filename is present
            options = options.create_if_missing(true);
            SqlitePoolOptions::new().max_connections(max_conn).connect_with(options).await?
        };

        // Configure SQLite for better performance and consistency
        sqlx::query("PRAGMA foreign_keys = ON;").execute(&pool).await?;
        sqlx::query("PRAGMA journal_mode = WAL;").execute(&pool).await?;
        sqlx::query("PRAGMA synchronous = NORMAL;").execute(&pool).await?;

        let dedup = create_sqlite_dedup_store(&pool);
        let store = Self { pool, dedup };

        // Run migrations
        let migration_runner = MigrationRunner::new(store.pool.clone());
        migration_runner.migrate().await?;

        Ok(store)
    }

    /// Create SqlStore from existing pool (for testing)
    pub fn from_pool(pool: SqlitePool) -> Self {
        let dedup = create_sqlite_dedup_store(&pool);
        Self { pool, dedup }
    }

    /// Run migrations manually
    pub async fn migrate(&self) -> StoreResult<()> {
        let migration_runner = MigrationRunner::new(self.pool.clone());
        migration_runner.migrate().await
    }

    pub fn dedup_store(&self) -> Arc<dyn DedupStore> {
        self.dedup.clone()
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

    async fn list_filtered(
        &self,
        filter: ActionListFilter,
        opts: Option<ActionListOptions>,
    ) -> CoreResult<Vec<ActionRecord>> {
        // Build dynamic SQL with parameters
        let mut sql = String::from(
            "SELECT trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version FROM actions",
        );
        let mut conds: Vec<String> = Vec::new();
        enum Bind {
            S(String),
            B(bool),
            T(chrono::DateTime<chrono::Utc>),
        }
        let mut binds: Vec<Bind> = Vec::new();

        if let Some(ref t) = filter.tenant {
            conds.push("trn LIKE ?".to_string());
            let like = format!("trn:openact:{}:%", t);
            binds.push(Bind::S(like));
        }
        if let Some(ref k) = filter.connector {
            conds.push("connector = ?".to_string());
            binds.push(Bind::S(k.as_str().to_string()));
        }
        if let Some(ref trn) = filter.connection_trn {
            conds.push("connection_trn = ?".to_string());
            binds.push(Bind::S(trn.as_str().to_string()));
        }
        if let Some(flag) = filter.mcp_enabled {
            conds.push("mcp_enabled = ?".to_string());
            binds.push(Bind::B(flag));
        }
        if let Some(ref prefix) = filter.name_prefix {
            conds.push("name LIKE ?".to_string());
            binds.push(Bind::S(format!("{}%", prefix)));
        }
        if let Some(ts) = filter.created_after {
            conds.push("created_at >= ?".to_string());
            binds.push(Bind::T(ts));
        }
        if let Some(ts) = filter.created_before {
            conds.push("created_at <= ?".to_string());
            binds.push(Bind::T(ts));
        }
        if let Some(ref q) = filter.q {
            conds.push("(LOWER(name) LIKE LOWER(?) OR LOWER(trn) LIKE LOWER(?))".to_string());
            let like = format!("%{}%", q);
            binds.push(Bind::S(like.clone()));
            binds.push(Bind::S(like));
        }

        // Governance allow/deny patterns -> SQL
        // allow_patterns: if specified and non-empty, tool (connector.name) must match at least one
        if let Some(ref allows) = filter.allow_patterns {
            if !allows.is_empty() {
                // If any allow is "*", skip adding constraint (allow all)
                if !allows.iter().any(|p| p == "*") {
                    let mut or_conds: Vec<String> = Vec::new();
                    for pat in allows {
                        if pat == "*" {
                            continue;
                        }
                        if let Some(prefix) = pat.strip_suffix(".*") {
                            or_conds.push("(connector = ?)".to_string());
                            binds.push(Bind::S(prefix.to_string()));
                        } else if let Some(suffix) = pat.strip_prefix("*.") {
                            or_conds.push("(name = ?)".to_string());
                            binds.push(Bind::S(suffix.to_string()));
                        } else if let Some((conn, name)) = pat.split_once('.') {
                            or_conds.push("(connector = ? AND name = ?)".to_string());
                            binds.push(Bind::S(conn.to_string()));
                            binds.push(Bind::S(name.to_string()));
                        } else {
                            or_conds.push("(connector = ?)".to_string());
                            binds.push(Bind::S(pat.to_string()));
                        }
                    }
                    if !or_conds.is_empty() {
                        conds.push(format!("({})", or_conds.join(" OR ")));
                    }
                }
            }
        }
        // deny_patterns: if specified, must not match any
        if let Some(ref denies) = filter.deny_patterns {
            if !denies.is_empty() {
                let mut or_conds: Vec<String> = Vec::new();
                for pat in denies {
                    if pat == "*" {
                        // Deny all
                        or_conds.clear();
                        or_conds.push("1=1".to_string());
                        break;
                    }
                    if let Some(prefix) = pat.strip_suffix(".*") {
                        or_conds.push("(connector = ?)".to_string());
                        binds.push(Bind::S(prefix.to_string()));
                    } else if let Some(suffix) = pat.strip_prefix("*.") {
                        or_conds.push("(name = ?)".to_string());
                        binds.push(Bind::S(suffix.to_string()));
                    } else if let Some((conn, name)) = pat.split_once('.') {
                        or_conds.push("(connector = ? AND name = ?)".to_string());
                        binds.push(Bind::S(conn.to_string()));
                        binds.push(Bind::S(name.to_string()));
                    } else {
                        or_conds.push("(connector = ?)".to_string());
                        binds.push(Bind::S(pat.to_string()));
                    }
                }
                if !or_conds.is_empty() {
                    conds.push(format!("NOT ({})", or_conds.join(" OR ")));
                }
            }
        }

        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }

        // Sorting
        let opts = opts.unwrap_or_default();
        sql.push_str(" ORDER BY ");
        match opts.sort_field.unwrap_or(ActionSortField::CreatedAt) {
            ActionSortField::CreatedAt => sql.push_str("created_at"),
            ActionSortField::Name => sql.push_str("name"),
            ActionSortField::Version => sql.push_str("version"),
        }
        if !opts.ascending {
            sql.push_str(" DESC");
        }

        // Pagination
        if let (Some(page), Some(page_size)) = (opts.page, opts.page_size) {
            let page = page.max(1);
            let page_size = page_size.max(1);
            let _offset = (page - 1) * page_size;
            sql.push_str(" LIMIT ? OFFSET ?");
            // We'll bind these as integers at the end
            // Using -1 placeholders is not necessary; we append binds later in order
        }

        let mut query = sqlx::query(&sql);
        for b in binds {
            match b {
                Bind::S(s) => {
                    query = query.bind(s);
                }
                Bind::B(v) => {
                    query = query.bind(v);
                }
                Bind::T(ts) => {
                    query = query.bind(ts);
                }
            }
        }
        if let (Some(page), Some(page_size)) = (opts.page, opts.page_size) {
            let page = page.max(1);
            let page_size = page_size.max(1);
            let offset = (page - 1) * page_size;
            query = query.bind(page_size as i64).bind(offset as i64);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(StoreError::Database)?;

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

    async fn list_filtered_paged(
        &self,
        filter: ActionListFilter,
        opts: ActionListOptions,
    ) -> CoreResult<ActionListResult> {
        // Build WHERE and binds once
        let mut where_sql = String::new();
        let mut conds: Vec<String> = Vec::new();
        enum Bind {
            S(String),
            B(bool),
            T(chrono::DateTime<chrono::Utc>),
        }
        let mut binds: Vec<Bind> = Vec::new();

        if let Some(ref t) = filter.tenant {
            conds.push("trn LIKE ?".to_string());
            let like = format!("trn:openact:{}:%", t);
            binds.push(Bind::S(like));
        }
        if let Some(ref k) = filter.connector {
            conds.push("connector = ?".to_string());
            binds.push(Bind::S(k.as_str().to_string()));
        }
        if let Some(ref trn) = filter.connection_trn {
            conds.push("connection_trn = ?".to_string());
            binds.push(Bind::S(trn.as_str().to_string()));
        }
        if let Some(flag) = filter.mcp_enabled {
            conds.push("mcp_enabled = ?".to_string());
            binds.push(Bind::B(flag));
        }
        if let Some(ref prefix) = filter.name_prefix {
            conds.push("name LIKE ?".to_string());
            binds.push(Bind::S(format!("{}%", prefix)));
        }
        if let Some(ts) = filter.created_after {
            conds.push("created_at >= ?".to_string());
            binds.push(Bind::T(ts));
        }
        if let Some(ts) = filter.created_before {
            conds.push("created_at <= ?".to_string());
            binds.push(Bind::T(ts));
        }
        if let Some(ref q) = filter.q {
            conds.push("(LOWER(name) LIKE LOWER(?) OR LOWER(trn) LIKE LOWER(?))".to_string());
            let like = format!("%{}%", q);
            binds.push(Bind::S(like.clone()));
            binds.push(Bind::S(like));
        }
        // Governance patterns for COUNT/SELECT
        if let Some(ref allows) = filter.allow_patterns {
            if !allows.is_empty() {
                if !allows.iter().any(|p| p == "*") {
                    let mut or_conds: Vec<String> = Vec::new();
                    for pat in allows {
                        if pat == "*" {
                            continue;
                        }
                        if let Some(prefix) = pat.strip_suffix(".*") {
                            or_conds.push("(connector = ?)".to_string());
                            binds.push(Bind::S(prefix.to_string()));
                        } else if let Some(suffix) = pat.strip_prefix("*.") {
                            or_conds.push("(name = ?)".to_string());
                            binds.push(Bind::S(suffix.to_string()));
                        } else if let Some((conn, name)) = pat.split_once('.') {
                            or_conds.push("(connector = ? AND name = ?)".to_string());
                            binds.push(Bind::S(conn.to_string()));
                            binds.push(Bind::S(name.to_string()));
                        } else {
                            or_conds.push("(connector = ?)".to_string());
                            binds.push(Bind::S(pat.to_string()));
                        }
                    }
                    if !or_conds.is_empty() {
                        conds.push(format!("({})", or_conds.join(" OR ")));
                    }
                }
            }
        }
        if let Some(ref denies) = filter.deny_patterns {
            if !denies.is_empty() {
                let mut or_conds: Vec<String> = Vec::new();
                for pat in denies {
                    if pat == "*" {
                        or_conds.clear();
                        or_conds.push("1=1".to_string());
                        break;
                    }
                    if let Some(prefix) = pat.strip_suffix(".*") {
                        or_conds.push("(connector = ?)".to_string());
                        binds.push(Bind::S(prefix.to_string()));
                    } else if let Some(suffix) = pat.strip_prefix("*.") {
                        or_conds.push("(name = ?)".to_string());
                        binds.push(Bind::S(suffix.to_string()));
                    } else if let Some((conn, name)) = pat.split_once('.') {
                        or_conds.push("(connector = ? AND name = ?)".to_string());
                        binds.push(Bind::S(conn.to_string()));
                        binds.push(Bind::S(name.to_string()));
                    } else {
                        or_conds.push("(connector = ?)".to_string());
                        binds.push(Bind::S(pat.to_string()));
                    }
                }
                if !or_conds.is_empty() {
                    conds.push(format!("NOT ({})", or_conds.join(" OR ")));
                }
            }
        }
        if !conds.is_empty() {
            where_sql.push_str(" WHERE ");
            where_sql.push_str(&conds.join(" AND "));
        }

        // Count query
        let count_sql = format!("SELECT COUNT(*) as cnt FROM actions{}", where_sql);
        let mut count_query = sqlx::query(&count_sql);
        for b in &binds {
            match b {
                Bind::S(s) => {
                    count_query = count_query.bind(s.clone());
                }
                Bind::B(v) => {
                    count_query = count_query.bind(*v);
                }
                Bind::T(ts) => {
                    count_query = count_query.bind(ts.clone());
                }
            }
        }
        let row = count_query.fetch_one(&self.pool).await.map_err(StoreError::Database)?;
        let total: i64 = row.get("cnt");

        // Records query with sort and pagination
        let mut sql = format!(
            "SELECT trn, connector, name, connection_trn, config_json, mcp_enabled, mcp_overrides_json, created_at, updated_at, version FROM actions{}",
            where_sql
        );
        sql.push_str(" ORDER BY ");
        match opts.sort_field.unwrap_or(ActionSortField::CreatedAt) {
            ActionSortField::CreatedAt => sql.push_str("created_at"),
            ActionSortField::Name => sql.push_str("name"),
            ActionSortField::Version => sql.push_str("version"),
        }
        if !opts.ascending {
            sql.push_str(" DESC");
        }
        if let (Some(page), Some(page_size)) = (opts.page, opts.page_size) {
            let page = page.max(1);
            let page_size = page_size.max(1);
            sql.push_str(" LIMIT ? OFFSET ?");
            let offset = (page - 1) * page_size;
            let mut query = sqlx::query(&sql);
            for b in &binds {
                match b {
                    Bind::S(s) => {
                        query = query.bind(s.clone());
                    }
                    Bind::B(v) => {
                        query = query.bind(*v);
                    }
                    Bind::T(ts) => {
                        query = query.bind(ts.clone());
                    }
                }
            }
            query = query.bind(page_size as i64).bind(offset as i64);
            let rows = query.fetch_all(&self.pool).await.map_err(StoreError::Database)?;
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
            return Ok(ActionListResult { records, total: total as u64 });
        }

        // No pagination: reuse existing list_filtered path
        let records = self.list_filtered(filter, Some(opts)).await?;
        Ok(ActionListResult { records, total: total as u64 })
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

        Ok(rows.into_iter().map(|row| row.get::<String, _>("trn")).collect())
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
                    INSERT OR IGNORE INTO auth_connections 
                    (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce,
                     refresh_token_encrypted, refresh_token_nonce, expires_at, token_type, scope,
                     extra_data_encrypted, extra_data_nonce, created_at, updated_at, version)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?)
                "#,
                )
                .bind(&new_conn.trn)
                .bind(&new_conn.tenant)
                .bind(&new_conn.provider)
                .bind(&new_conn.user_id)
                .bind(&access_token_encrypted)
                .bind(&access_token_nonce)
                .bind(&refresh_token_encrypted)
                .bind(&refresh_token_nonce)
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

#[async_trait]
impl OrchestratorRunStore for SqlStore {
    async fn insert_run(&self, run: &OrchestratorRunRecord) -> CoreResult<()> {
        let result_json = serialize_optional_json(run.result.as_ref())?;
        let error_json = serialize_optional_json(run.error.as_ref())?;
        let metadata_json = serialize_optional_json(run.metadata.as_ref())?;

        sqlx::query(
            r#"
            INSERT INTO orchestrator_runs (
                run_id, command_id, tenant, action_trn, status, phase, trace_id, correlation_id,
                heartbeat_at, deadline_at, status_ttl_seconds, next_poll_at, poll_attempts,
                external_ref, result_json, error_json, metadata_json, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&run.run_id)
        .bind(&run.command_id)
        .bind(&run.tenant)
        .bind(run.action_trn.as_str())
        .bind(run.status.as_str())
        .bind(&run.phase)
        .bind(&run.trace_id)
        .bind(&run.correlation_id)
        .bind(run.heartbeat_at)
        .bind(run.deadline_at)
        .bind(run.status_ttl_seconds)
        .bind(run.next_poll_at)
        .bind(run.poll_attempts)
        .bind(&run.external_ref)
        .bind(result_json)
        .bind(error_json)
        .bind(metadata_json)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn get_run(&self, run_id: &str) -> CoreResult<Option<OrchestratorRunRecord>> {
        let row = sqlx::query(
            r#"
            SELECT *
            FROM orchestrator_runs
            WHERE run_id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        if let Some(row) = row {
            let record = map_run_row(&row)?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    async fn update_status(
        &self,
        run_id: &str,
        status: OrchestratorRunStatus,
        phase: Option<String>,
        result: Option<JsonValue>,
        error: Option<JsonValue>,
    ) -> CoreResult<()> {
        let result_json = serialize_optional_json(result.as_ref())?;
        let error_json = serialize_optional_json(error.as_ref())?;

        sqlx::query(
            r#"
            UPDATE orchestrator_runs
            SET status = ?, phase = ?, result_json = ?, error_json = ?, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(&phase)
        .bind(result_json)
        .bind(error_json)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn refresh_heartbeat(
        &self,
        run_id: &str,
        heartbeat_at: DateTime<Utc>,
        deadline_at: Option<DateTime<Utc>>,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE orchestrator_runs
            SET heartbeat_at = ?, deadline_at = ?, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = ?
            "#,
        )
        .bind(heartbeat_at)
        .bind(deadline_at)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn update_poll_schedule(
        &self,
        run_id: &str,
        next_poll_at: Option<DateTime<Utc>>,
        poll_attempts: i32,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE orchestrator_runs
            SET next_poll_at = ?, poll_attempts = ?, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = ?
            "#,
        )
        .bind(next_poll_at)
        .bind(poll_attempts)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn update_metadata_external(
        &self,
        run_id: &str,
        metadata: Option<JsonValue>,
        external_ref: Option<String>,
    ) -> CoreResult<()> {
        let metadata_json = serialize_optional_json(metadata.as_ref())?;

        sqlx::query(
            r#"
            UPDATE orchestrator_runs
            SET metadata_json = ?, external_ref = ?, updated_at = CURRENT_TIMESTAMP
            WHERE run_id = ?
            "#,
        )
        .bind(metadata_json)
        .bind(external_ref)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn list_for_timeout(
        &self,
        heartbeat_cutoff: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorRunRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM orchestrator_runs
            WHERE status = 'running'
              AND (heartbeat_at <= ? OR (deadline_at IS NOT NULL AND deadline_at <= ?))
            ORDER BY heartbeat_at ASC
            LIMIT ?
            "#,
        )
        .bind(heartbeat_cutoff)
        .bind(heartbeat_cutoff)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let records = rows
            .into_iter()
            .map(|row| map_run_row(&row))
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(records)
    }

    async fn list_due_for_poll(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorRunRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM orchestrator_runs
            WHERE status = 'running'
              AND next_poll_at IS NOT NULL
              AND next_poll_at <= ?
            ORDER BY next_poll_at ASC
            LIMIT ?
            "#,
        )
        .bind(as_of)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let records = rows
            .into_iter()
            .map(|row| map_run_row(&row))
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(records)
    }
}

#[async_trait]
impl OrchestratorOutboxStore for SqlStore {
    async fn enqueue(&self, insert: OrchestratorOutboxInsert) -> CoreResult<i64> {
        let payload_json =
            serde_json::to_string(&insert.payload).map_err(StoreError::Serialization)?;

        let result = sqlx::query(
            r#"
            INSERT INTO orchestrator_outbox (
                run_id, protocol, payload_json, attempts, next_attempt_at, last_error,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&insert.run_id)
        .bind(&insert.protocol)
        .bind(payload_json)
        .bind(insert.attempts)
        .bind(insert.next_attempt_at)
        .bind(&insert.last_error)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(result.last_insert_rowid())
    }

    async fn fetch_ready(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorOutboxRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM orchestrator_outbox
            WHERE delivered_at IS NULL
              AND next_attempt_at <= ?
            ORDER BY next_attempt_at ASC, id ASC
            LIMIT ?
            "#,
        )
        .bind(as_of)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        let records = rows
            .into_iter()
            .map(|row| map_outbox_row(&row))
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(records)
    }

    async fn mark_delivered(&self, id: i64, delivered_at: DateTime<Utc>) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE orchestrator_outbox
            SET delivered_at = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(delivered_at)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }

    async fn mark_retry(
        &self,
        id: i64,
        next_attempt_at: DateTime<Utc>,
        attempts: i32,
        last_error: Option<String>,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE orchestrator_outbox
            SET next_attempt_at = ?, attempts = ?, last_error = ?, updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(next_attempt_at)
        .bind(attempts)
        .bind(last_error)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Database)?;

        Ok(())
    }
}

fn serialize_optional_json(value: Option<&JsonValue>) -> Result<Option<String>, StoreError> {
    value.map(|v| serde_json::to_string(v).map_err(StoreError::Serialization)).transpose()
}

fn deserialize_optional_json(value: Option<String>) -> Result<Option<JsonValue>, StoreError> {
    value.map(|json| serde_json::from_str(&json).map_err(StoreError::Serialization)).transpose()
}

fn map_run_row(row: &SqliteRow) -> Result<OrchestratorRunRecord, StoreError> {
    let status_str: String = row.get("status");
    let status = OrchestratorRunStatus::from_str(&status_str).map_err(|_| {
        StoreError::Validation(format!("unknown orchestrator run status: {}", status_str))
    })?;

    let poll_attempts_i64: i64 = row.get("poll_attempts");
    let poll_attempts = i32::try_from(poll_attempts_i64)
        .map_err(|_| StoreError::Validation("poll_attempts exceeds i32 range".into()))?;

    let result = deserialize_optional_json(row.get::<Option<String>, _>("result_json"))?;
    let error = deserialize_optional_json(row.get::<Option<String>, _>("error_json"))?;
    let metadata = deserialize_optional_json(row.get::<Option<String>, _>("metadata_json"))?;

    Ok(OrchestratorRunRecord {
        command_id: row.get::<String, _>("command_id"),
        run_id: row.get::<String, _>("run_id"),
        tenant: row.get::<String, _>("tenant"),
        action_trn: Trn::new(row.get::<String, _>("action_trn")),
        status,
        phase: row.get::<Option<String>, _>("phase"),
        trace_id: row.get::<String, _>("trace_id"),
        correlation_id: row.get::<Option<String>, _>("correlation_id"),
        heartbeat_at: row.get::<DateTime<Utc>, _>("heartbeat_at"),
        deadline_at: row.get::<Option<DateTime<Utc>>, _>("deadline_at"),
        status_ttl_seconds: row.get::<Option<i64>, _>("status_ttl_seconds"),
        next_poll_at: row.get::<Option<DateTime<Utc>>, _>("next_poll_at"),
        poll_attempts,
        external_ref: row.get::<Option<String>, _>("external_ref"),
        result,
        error,
        metadata,
        created_at: row.get::<DateTime<Utc>, _>("created_at"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
    })
}

fn map_outbox_row(row: &SqliteRow) -> Result<OrchestratorOutboxRecord, StoreError> {
    let attempts_i64: i64 = row.get("attempts");
    let attempts = i32::try_from(attempts_i64)
        .map_err(|_| StoreError::Validation("attempts exceeds i32 range".into()))?;

    let payload_json: String = row.get("payload_json");
    let payload = serde_json::from_str(&payload_json).map_err(StoreError::Serialization)?;

    Ok(OrchestratorOutboxRecord {
        id: row.get::<i64, _>("id"),
        run_id: row.get::<Option<String>, _>("run_id"),
        protocol: row.get::<String, _>("protocol"),
        payload,
        attempts,
        next_attempt_at: row.get::<DateTime<Utc>, _>("next_attempt_at"),
        last_error: row.get::<Option<String>, _>("last_error"),
        created_at: row.get::<DateTime<Utc>, _>("created_at"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at"),
        delivered_at: row.get::<Option<DateTime<Utc>>, _>("delivered_at"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[cfg(not(feature = "encryption"))]
    #[tokio::test]
    async fn compare_and_swap_insert_stores_plain_tokens_when_encryption_disabled() {
        let store = SqlStore::new("sqlite::memory:").await.expect("create store");

        let mut auth = AuthConnection::new("tenant", "provider", "user", "access-token");
        auth.refresh_token = Some("refresh-token".to_string());
        auth.scope = Some("scope".to_string());
        auth.extra = json!({"metadata": 1});
        auth.expires_at = Some(Utc::now());
        let trn = auth.trn.clone();

        let inserted = store
            .compare_and_swap(&trn, None, Some(&auth))
            .await
            .expect("compare_and_swap succeeds");
        assert!(inserted);

        let row = sqlx::query(
            "SELECT access_token_encrypted, access_token_nonce, refresh_token_encrypted, refresh_token_nonce FROM auth_connections WHERE trn = ?",
        )
        .bind(&trn)
        .fetch_one(&store.pool)
        .await
        .expect("row present");

        let at: String = row.get("access_token_encrypted");
        let at_nonce: String = row.get("access_token_nonce");
        let rt: Option<String> = row.try_get("refresh_token_encrypted").ok();
        let rt_nonce: String = row.get("refresh_token_nonce");

        assert_eq!(at, "access-token");
        assert!(rt.as_deref().is_some_and(|v| v == "refresh-token"));
        assert!(at_nonce.is_empty());
        assert!(rt_nonce.is_empty());
    }
}
