//! Storage Service Layer
//!
//! Provides a unified database service interface, integrating ConnectionRepository and TaskRepository

use anyhow::{Result, anyhow};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::OnceCell;

use super::{ConnectionRepository, DatabaseManager, TaskRepository};
use crate::executor::{ExecutionResult, Executor};
use crate::models::AuthConnection;
use crate::models::{ConnectionConfig, TaskConfig};
use crate::store::{AuthConnectionTrn, ConnectionStore};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use sqlx::{Row, sqlite::SqliteRow};

// Global storage service instance
static STORAGE_SERVICE: OnceCell<Arc<StorageService>> = OnceCell::const_new();
static INJECTED_STORAGE_SERVICE: OnceCell<TokioMutex<Option<Arc<StorageService>>>> =
    OnceCell::const_new();

/// Test-only: inject a global storage service instance
pub async fn set_global_storage_service_for_tests(service: Arc<StorageService>) {
    let slot = INJECTED_STORAGE_SERVICE
        .get_or_init(|| async { TokioMutex::new(None) })
        .await;
    let mut guard = slot.lock().await;
    *guard = Some(service);
}

/// Test-only: reset all global state to allow fresh initialization per test
pub async fn reset_global_storage_for_tests() {
    if let Some(slot) = INJECTED_STORAGE_SERVICE.get() {
        let mut guard = slot.lock().await;
        *guard = None;
    }
    // Also reset the main STORAGE_SERVICE if it was initialized
    // Note: OnceCell doesn't have a reset method, but the injection takes precedence
}

/// Storage Service
pub struct StorageService {
    db_manager: DatabaseManager,
    connection_repo: ConnectionRepository,
    task_repo: TaskRepository,
    // Simple execution context cache: task_trn -> ((conn, task), timestamp)
    exec_cache: Arc<tokio::sync::Mutex<HashMap<String, ((ConnectionConfig, TaskConfig), Instant)>>>,
    // TTL cache for connections and tasks
    connection_cache: Arc<tokio::sync::Mutex<HashMap<String, (ConnectionConfig, Instant)>>>,
    task_cache: Arc<tokio::sync::Mutex<HashMap<String, (TaskConfig, Instant)>>>,
    // Cache metrics
    exec_cache_lookups: AtomicU64,
    exec_cache_hits: AtomicU64,
    conn_cache_lookups: AtomicU64,
    conn_cache_hits: AtomicU64,
    task_cache_lookups: AtomicU64,
    task_cache_hits: AtomicU64,
}

impl StorageService {
    /// Create a storage service
    pub fn new(db_manager: DatabaseManager) -> Self {
        let connection_repo = db_manager.connection_repository();
        let task_repo = TaskRepository::new(db_manager.pool().clone());

        Self {
            db_manager,
            connection_repo,
            task_repo,
            exec_cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            connection_cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            task_cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            exec_cache_lookups: AtomicU64::new(0),
            exec_cache_hits: AtomicU64::new(0),
            conn_cache_lookups: AtomicU64::new(0),
            conn_cache_hits: AtomicU64::new(0),
            task_cache_lookups: AtomicU64::new(0),
            task_cache_hits: AtomicU64::new(0),
        }
    }

    /// Initialize storage service from environment variables
    pub async fn from_env() -> Result<Self> {
        let db_manager = DatabaseManager::from_env().await?;
        Ok(Self::new(db_manager))
    }

    /// Get global storage service instance
    pub async fn global() -> Arc<StorageService> {
        if let Some(slot) = INJECTED_STORAGE_SERVICE.get() {
            let injected_opt = slot.lock().await.clone();
            if let Some(injected) = injected_opt {
                return injected;
            }
        }
        if let Some(service) = STORAGE_SERVICE.get() {
            return service.clone();
        }

        let service = Arc::new(
            Self::from_env()
                .await
                .expect("Failed to initialize storage service"),
        );
        let _ = STORAGE_SERVICE.set(service.clone());
        service
    }

    /// Get database manager
    pub fn database(&self) -> &DatabaseManager {
        &self.db_manager
    }

    /// Get connection repository
    pub fn connections(&self) -> &ConnectionRepository {
        &self.connection_repo
    }

    /// Get task repository
    pub fn tasks(&self) -> &TaskRepository {
        &self.task_repo
    }

    // === Connection CRUD Operations ===

    /// Create or update connection configuration
    pub async fn upsert_connection(&self, connection: &ConnectionConfig) -> Result<()> {
        self.connection_repo.upsert(connection).await?;
        // Invalidate cache
        self.invalidate_connection_cache(&connection.trn).await;
        Ok(())
    }

    /// Get connection configuration by TRN
    pub async fn get_connection(&self, trn: &str) -> Result<Option<ConnectionConfig>> {
        self.connection_repo.get_by_trn(trn).await
    }

    /// List connection configurations
    pub async fn list_connections(
        &self,
        auth_type: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ConnectionConfig>> {
        self.connection_repo.list(auth_type, limit, offset).await
    }

    /// Delete connection configuration
    pub async fn delete_connection(&self, trn: &str) -> Result<bool> {
        // First delete related tasks
        let deleted_tasks = self.task_repo.delete_by_connection(trn).await?;
        tracing::info!("Deleted {} tasks for connection {}", deleted_tasks, trn);

        // Then delete connection
        let ok = self.connection_repo.delete(trn).await?;
        // Invalidate cache (all tasks associated with this connection)
        self.invalidate_cache_by_connection(trn).await;
        self.invalidate_connection_cache(trn).await;
        Ok(ok)
    }

    /// Count connections
    pub async fn count_connections(&self, auth_type: Option<&str>) -> Result<i64> {
        self.connection_repo.count_by_type(auth_type).await
    }

    // === Task CRUD Operations ===

    /// Create or update task configuration
    pub async fn upsert_task(&self, task: &TaskConfig) -> Result<()> {
        // Validate if the associated connection exists
        if !self
            .task_repo
            .validate_connection_exists(&task.connection_trn)
            .await?
        {
            return Err(anyhow!("Connection not found: {}", task.connection_trn));
        }

        self.task_repo.upsert(task).await?;
        // Invalidate task cache
        self.invalidate_cache_by_task(&task.trn).await;
        self.invalidate_task_cache(&task.trn).await;
        Ok(())
    }

    /// Get task configuration by TRN
    pub async fn get_task(&self, trn: &str) -> Result<Option<TaskConfig>> {
        self.task_repo.get_by_trn(trn).await
    }

    /// List task configurations
    pub async fn list_tasks(
        &self,
        connection_trn: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskConfig>> {
        self.task_repo.list(connection_trn, limit, offset).await
    }

    /// Delete task configuration
    pub async fn delete_task(&self, trn: &str) -> Result<bool> {
        let ok = self.task_repo.delete(trn).await?;
        if ok {
            self.invalidate_cache_by_task(trn).await;
            self.invalidate_task_cache(trn).await;
        }
        Ok(ok)
    }

    /// Count tasks
    pub async fn count_tasks(&self, connection_trn: Option<&str>) -> Result<i64> {
        self.task_repo.count_by_connection(connection_trn).await
    }

    // === Advanced Operations ===

    /// Get complete execution context (connection + task) with TTL cache
    pub async fn get_execution_context(
        &self,
        task_trn: &str,
    ) -> Result<Option<(ConnectionConfig, TaskConfig)>> {
        self.exec_cache_lookups.fetch_add(1, Ordering::Relaxed);
        let cached = self
            .get_cached_execution_context(task_trn, Duration::from_secs(60))
            .await;
        if let Some(ctx) = cached {
            self.exec_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(ctx));
        }

        let task = match self.get_task(task_trn).await? {
            Some(task) => task,
            None => return Ok(None),
        };

        let connection = match self.get_connection(&task.connection_trn).await? {
            Some(conn) => conn,
            None => return Err(anyhow!("Connection not found for task: {}", task_trn)),
        };

        let pair = (connection, task);
        self.put_cached_execution_context(task_trn.to_string(), pair.clone())
            .await;
        Ok(Some(pair))
    }

    /// Get connection (with TTL cache) — interface reserved, Phase 0 directly calls storage
    pub async fn get_connection_cached(
        &self,
        trn: &str,
        _ttl: std::time::Duration,
    ) -> Result<Option<ConnectionConfig>> {
        let ttl = _ttl;
        self.conn_cache_lookups.fetch_add(1, Ordering::Relaxed);
        // First check cache
        if let Some(cached) = self.get_cached_connection(trn, ttl).await {
            self.conn_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(cached));
        }
        // Read storage and write to cache
        let conn = self.get_connection(trn).await?;
        if let Some(ref c) = conn {
            self.put_cached_connection(trn.to_string(), c.clone()).await;
        }
        Ok(conn)
    }

    /// Get task (with TTL cache) — interface reserved, Phase 0 directly calls storage
    pub async fn get_task_cached(
        &self,
        trn: &str,
        _ttl: std::time::Duration,
    ) -> Result<Option<TaskConfig>> {
        let ttl = _ttl;
        self.task_cache_lookups.fetch_add(1, Ordering::Relaxed);
        if let Some(cached) = self.get_cached_task(trn, ttl).await {
            self.task_cache_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(cached));
        }
        let task = self.get_task(trn).await?;
        if let Some(ref t) = task {
            self.put_cached_task(trn.to_string(), t.clone()).await;
        }
        Ok(task)
    }

    async fn get_cached_execution_context(
        &self,
        task_trn: &str,
        ttl: Duration,
    ) -> Option<(ConnectionConfig, TaskConfig)> {
        let mut guard = self.exec_cache.lock().await;
        if let Some(((conn, task), ts)) = guard.get(task_trn) {
            if Instant::now().duration_since(*ts) < ttl {
                return Some((conn.clone(), task.clone()));
            } else {
                guard.remove(task_trn);
            }
        }
        None
    }

    async fn put_cached_execution_context(
        &self,
        task_trn: String,
        value: (ConnectionConfig, TaskConfig),
    ) {
        let mut guard = self.exec_cache.lock().await;
        guard.insert(task_trn, (value, Instant::now()));
    }

    async fn invalidate_cache_by_connection(&self, connection_trn: &str) {
        let mut guard = self.exec_cache.lock().await;
        guard.retain(|_, ((conn, _), _)| conn.trn != connection_trn);
    }

    async fn invalidate_cache_by_task(&self, task_trn: &str) {
        let mut guard = self.exec_cache.lock().await;
        guard.remove(task_trn);
    }

    async fn get_cached_connection(&self, trn: &str, ttl: Duration) -> Option<ConnectionConfig> {
        let mut guard = self.connection_cache.lock().await;
        if let Some((conn, ts)) = guard.get(trn) {
            if Instant::now().duration_since(*ts) < ttl {
                return Some(conn.clone());
            } else {
                guard.remove(trn);
            }
        }
        None
    }

    async fn put_cached_connection(&self, trn: String, conn: ConnectionConfig) {
        let mut guard = self.connection_cache.lock().await;
        guard.insert(trn, (conn, Instant::now()));
    }

    async fn invalidate_connection_cache(&self, trn: &str) {
        let mut guard = self.connection_cache.lock().await;
        guard.remove(trn);
    }

    async fn get_cached_task(&self, trn: &str, ttl: Duration) -> Option<TaskConfig> {
        let mut guard = self.task_cache.lock().await;
        if let Some((task, ts)) = guard.get(trn) {
            if Instant::now().duration_since(*ts) < ttl {
                return Some(task.clone());
            } else {
                guard.remove(trn);
            }
        }
        None
    }

    async fn put_cached_task(&self, trn: String, task: TaskConfig) {
        let mut guard = self.task_cache.lock().await;
        guard.insert(trn, (task, Instant::now()));
    }

    async fn invalidate_task_cache(&self, trn: &str) {
        let mut guard = self.task_cache.lock().await;
        guard.remove(trn);
    }

    /// Cache metrics
    pub async fn get_cache_stats(&self) -> CacheStats {
        let exec_lookups = self.exec_cache_lookups.load(Ordering::Relaxed);
        let exec_hits = self.exec_cache_hits.load(Ordering::Relaxed);
        let conn_lookups = self.conn_cache_lookups.load(Ordering::Relaxed);
        let conn_hits = self.conn_cache_hits.load(Ordering::Relaxed);
        let task_lookups = self.task_cache_lookups.load(Ordering::Relaxed);
        let task_hits = self.task_cache_hits.load(Ordering::Relaxed);
        CacheStats {
            exec_lookups,
            exec_hits,
            exec_hit_rate: if exec_lookups > 0 {
                exec_hits as f64 / exec_lookups as f64
            } else {
                0.0
            },
            conn_lookups,
            conn_hits,
            conn_hit_rate: if conn_lookups > 0 {
                conn_hits as f64 / conn_lookups as f64
            } else {
                0.0
            },
            task_lookups,
            task_hits,
            task_hit_rate: if task_lookups > 0 {
                task_hits as f64 / task_lookups as f64
            } else {
                0.0
            },
            connection_cache_size: self.connection_cache.lock().await.len() as u64,
            task_cache_size: self.task_cache.lock().await.len() as u64,
            exec_cache_size: self.exec_cache.lock().await.len() as u64,
        }
    }

    /// Batch import connections and tasks
    pub async fn import_configurations(
        &self,
        connections: Vec<ConnectionConfig>,
        tasks: Vec<TaskConfig>,
    ) -> Result<(usize, usize)> {
        let mut imported_connections = 0;
        let mut imported_tasks = 0;

        // Import connections
        for connection in connections {
            match self.upsert_connection(&connection).await {
                Ok(_) => imported_connections += 1,
                Err(e) => tracing::warn!("Failed to import connection {}: {}", connection.trn, e),
            }
        }

        // Import tasks
        for task in tasks {
            match self.upsert_task(&task).await {
                Ok(_) => imported_tasks += 1,
                Err(e) => tracing::warn!("Failed to import task {}: {}", task.trn, e),
            }
        }

        Ok((imported_connections, imported_tasks))
    }

    /// Export all configurations
    pub async fn export_configurations(&self) -> Result<(Vec<ConnectionConfig>, Vec<TaskConfig>)> {
        let connections = self.list_connections(None, None, None).await?;
        let tasks = self.list_tasks(None, None, None).await?;
        Ok((connections, tasks))
    }

    /// Health check
    pub async fn health_check(&self) -> Result<()> {
        self.db_manager.health_check().await
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<StorageStats> {
        let db_stats = self.db_manager.get_stats().await?;

        // Count connections by authentication type
        let api_key_connections = self.count_connections(Some("api_key")).await?;
        let basic_connections = self.count_connections(Some("basic")).await?;
        let oauth2_cc_connections = self
            .count_connections(Some("oauth2_client_credentials"))
            .await?;
        let oauth2_ac_connections = self
            .count_connections(Some("oauth2_authorization_code"))
            .await?;

        Ok(StorageStats {
            total_connections: db_stats.connections_count,
            total_tasks: db_stats.tasks_count,
            total_auth_connections: db_stats.auth_connections_count,
            api_key_connections,
            basic_connections,
            oauth2_cc_connections,
            oauth2_ac_connections,
        })
    }

    /// Clean up expired data
    pub async fn cleanup(&self) -> Result<CleanupResult> {
        let expired_auth_connections = self.db_manager.cleanup_expired_auth_connections().await?;

        Ok(CleanupResult {
            expired_auth_connections,
        })
    }

    /// Execute by TRN
    pub async fn execute_by_trn(&self, task_trn: &str) -> Result<ExecutionResult> {
        let (conn, task) = self
            .get_execution_context(task_trn)
            .await?
            .ok_or_else(|| anyhow!("Task not found: {}", task_trn))?;
        let executor = Executor::new();
        executor.execute(&conn, &task).await
    }
}

// === AuthConnection helpers (enc/dec and row mapping) ===
impl StorageService {
    fn enc_service(&self) -> Option<&crate::store::encryption::FieldEncryption> {
        self.db_manager.encryption().as_ref()
    }

    fn encrypt_field(&self, data: &str) -> Result<(String, String)> {
        if let Some(enc) = self.enc_service() {
            let encrypted = enc.encrypt_field(data)?;
            Ok((encrypted.data, encrypted.nonce))
        } else {
            Ok((
                general_purpose::STANDARD.encode(data),
                "no-encryption".to_string(),
            ))
        }
    }

    fn decrypt_field(&self, data: &str, nonce: &str, key_version: Option<u32>) -> Result<String> {
        if let Some(enc) = self.enc_service() {
            let ef = crate::store::encryption::EncryptedField {
                data: data.to_string(),
                nonce: nonce.to_string(),
                key_version: key_version.unwrap_or(1),
            };
            enc.decrypt_field(&ef)
        } else {
            let decoded = general_purpose::STANDARD
                .decode(data)
                .map_err(|e| anyhow!("Failed to decode data: {}", e))?;
            String::from_utf8(decoded).map_err(|e| anyhow!("Invalid UTF-8 in data: {}", e))
        }
    }

    fn row_to_auth_connection(&self, row: &SqliteRow) -> Result<AuthConnection> {
        let access_token_encrypted: String = row.get("access_token_encrypted");
        let access_token_nonce: String = row.get("access_token_nonce");
        let key_version: Option<u32> = row.try_get("key_version").ok();
        let access_token =
            self.decrypt_field(&access_token_encrypted, &access_token_nonce, key_version)?;

        let refresh_token = if let (Ok(enc), Ok(nonce)) = (
            row.try_get::<String, _>("refresh_token_encrypted"),
            row.try_get::<String, _>("refresh_token_nonce"),
        ) {
            if !enc.is_empty() && !nonce.is_empty() {
                Some(self.decrypt_field(&enc, &nonce, key_version)?)
            } else {
                None
            }
        } else {
            None
        };

        let extra = if let (Ok(enc), Ok(nonce)) = (
            row.try_get::<String, _>("extra_data_encrypted"),
            row.try_get::<String, _>("extra_data_nonce"),
        ) {
            if !enc.is_empty() && !nonce.is_empty() {
                let decrypted = self.decrypt_field(&enc, &nonce, key_version)?;
                serde_json::from_str(&decrypted).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        } else {
            serde_json::Value::Null
        };

        let tenant: String = row.get("tenant");
        let provider: String = row.get("provider");
        let user_id: String = row.get("user_id");
        let trn = AuthConnectionTrn::new(tenant, provider, user_id)?;

        Ok(AuthConnection {
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
}

#[async_trait]
impl ConnectionStore for StorageService {
    async fn get(&self, connection_ref: &str) -> Result<Option<AuthConnection>> {
        let row = sqlx::query("SELECT * FROM auth_connections WHERE trn = ?")
            .bind(connection_ref)
            .fetch_optional(self.db_manager.pool())
            .await?;
        if let Some(row) = row {
            Ok(Some(self.row_to_auth_connection(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, connection_ref: &str, connection: &AuthConnection) -> Result<()> {
        let (access_token_encrypted, access_token_nonce) =
            self.encrypt_field(&connection.access_token)?;
        let (refresh_token_encrypted, refresh_token_nonce) =
            if let Some(ref token) = connection.refresh_token {
                let (e, n) = self.encrypt_field(token)?;
                (Some(e), Some(n))
            } else {
                (None, None)
            };
        let (extra_data_encrypted, extra_data_nonce) =
            if connection.extra != serde_json::Value::Null {
                let json = serde_json::to_string(&connection.extra)?;
                let (e, n) = self.encrypt_field(&json)?;
                (Some(e), Some(n))
            } else {
                (None, None)
            };

        let existing = self.get(connection_ref).await?;
        if existing.is_some() {
            sqlx::query(
                r#"
                UPDATE auth_connections SET
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
            .execute(self.db_manager.pool())
            .await?;
        } else {
            sqlx::query(
                r#"
                INSERT INTO auth_connections 
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
            .execute(self.db_manager.pool())
            .await?;
        }
        Ok(())
    }

    async fn delete(&self, connection_ref: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM auth_connections WHERE trn = ?")
            .bind(connection_ref)
            .execute(self.db_manager.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> Result<bool> {
        let mut tx = self.db_manager.pool().begin().await?;
        let current = sqlx::query("SELECT * FROM auth_connections WHERE trn = ?")
            .bind(connection_ref)
            .fetch_optional(&mut *tx)
            .await?;
        let current_conn = if let Some(row) = current {
            Some(self.row_to_auth_connection(&row)?)
        } else {
            None
        };
        let matches = match (expected, &current_conn) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };
        if !matches {
            tx.rollback().await?;
            return Ok(false);
        }

        match new_connection {
            Some(new_conn) => {
                // Reuse put logic within tx (simplified: touch updated_at if exists)
                if current_conn.is_some() {
                    sqlx::query("UPDATE auth_connections SET updated_at = CURRENT_TIMESTAMP, version = version + 1 WHERE trn = ?")
                        .bind(connection_ref)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    // Minimal insert with placeholders; caller should follow with put to set full fields
                    sqlx::query("INSERT INTO auth_connections (trn, tenant, provider, user_id, access_token_encrypted, access_token_nonce, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
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
                sqlx::query("DELETE FROM auth_connections WHERE trn = ?")
                    .bind(connection_ref)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;
        Ok(true)
    }

    async fn list_refs(&self) -> Result<Vec<String>> {
        let refs = sqlx::query_scalar::<_, String>(
            "SELECT trn FROM auth_connections ORDER BY created_at DESC",
        )
        .fetch_all(self.db_manager.pool())
        .await?;
        Ok(refs)
    }

    async fn cleanup_expired(&self) -> Result<u64> {
        self.db_manager.cleanup_expired_auth_connections().await
    }

    async fn count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_connections")
            .fetch_one(self.db_manager.pool())
            .await?;
        Ok(count as u64)
    }
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Default)]
pub struct StorageStats {
    pub total_connections: i64,
    pub total_tasks: i64,
    pub total_auth_connections: i64,
    pub api_key_connections: i64,
    pub basic_connections: i64,
    pub oauth2_cc_connections: i64,
    pub oauth2_ac_connections: i64,
}

/// Cleanup result
#[derive(Debug, Clone, Serialize)]
pub struct CleanupResult {
    pub expired_auth_connections: u64,
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Default)]
pub struct CacheStats {
    pub exec_lookups: u64,
    pub exec_hits: u64,
    pub exec_hit_rate: f64,
    pub conn_lookups: u64,
    pub conn_hits: u64,
    pub conn_hit_rate: f64,
    pub task_lookups: u64,
    pub task_hits: u64,
    pub task_hit_rate: f64,
    pub connection_cache_size: u64,
    pub task_cache_size: u64,
    pub exec_cache_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApiKeyAuthParameters, AuthorizationType};

    async fn create_test_service() -> StorageService {
        let database_url = "sqlite::memory:";
        let db_manager = DatabaseManager::new(database_url).await.unwrap();
        StorageService::new(db_manager)
    }

    fn create_test_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:openact:default:connection/test".to_string(),
            "Test Connection".to_string(),
            AuthorizationType::ApiKey,
        );

        connection.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "test-key".to_string(),
        });

        connection
    }

    fn create_test_task(connection_trn: &str) -> TaskConfig {
        TaskConfig::new(
            "trn:openact:default:task/test".to_string(),
            "Test Task".to_string(),
            connection_trn.to_string(),
            "https://api.example.com/users".to_string(),
            "GET".to_string(),
        )
    }

    #[tokio::test]
    async fn test_service_crud_operations() {
        let service = create_test_service().await;

        // Test connection CRUD
        let connection = create_test_connection();
        service.upsert_connection(&connection).await.unwrap();

        let retrieved = service.get_connection(&connection.trn).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().trn, connection.trn);

        // Test task CRUD
        let task = create_test_task(&connection.trn);
        service.upsert_task(&task).await.unwrap();

        let retrieved_task = service.get_task(&task.trn).await.unwrap();
        assert!(retrieved_task.is_some());
        let retrieved_task = retrieved_task.unwrap();
        assert_eq!(retrieved_task.trn, task.trn);

        // Test execution context
        let context = service.get_execution_context(&task.trn).await.unwrap();
        assert!(context.is_some());
        let (conn, tsk) = context.unwrap();
        assert_eq!(conn.trn, connection.trn);
        assert_eq!(tsk.trn, task.trn);
    }
}
