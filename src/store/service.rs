//! 存储服务层
//!
//! 提供统一的数据库服务接口，集成ConnectionRepository和TaskRepository

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;

use super::{ConnectionRepository, DatabaseManager, TaskRepository};
use crate::executor::{ExecutionResult, Executor};
use crate::models::{ConnectionConfig, TaskConfig};

// 全局存储服务实例
static STORAGE_SERVICE: OnceCell<Arc<StorageService>> = OnceCell::const_new();

/// 存储服务
pub struct StorageService {
    db_manager: DatabaseManager,
    connection_repo: ConnectionRepository,
    task_repo: TaskRepository,
    // 简易执行上下文缓存：task_trn -> ((conn, task), timestamp)
    exec_cache: Arc<tokio::sync::Mutex<HashMap<String, ((ConnectionConfig, TaskConfig), Instant)>>>,
}

impl StorageService {
    /// 创建存储服务
    pub fn new(db_manager: DatabaseManager) -> Self {
        let connection_repo = db_manager.connection_repository();
        let task_repo = TaskRepository::new(db_manager.pool().clone());

        Self {
            db_manager,
            connection_repo,
            task_repo,
            exec_cache: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// 从环境变量初始化存储服务
    pub async fn from_env() -> Result<Self> {
        let db_manager = DatabaseManager::from_env().await?;
        Ok(Self::new(db_manager))
    }

    /// 获取全局存储服务实例
    pub async fn global() -> Arc<StorageService> {
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

    /// 获取数据库管理器
    pub fn database(&self) -> &DatabaseManager {
        &self.db_manager
    }

    /// 获取连接仓储
    pub fn connections(&self) -> &ConnectionRepository {
        &self.connection_repo
    }

    /// 获取任务仓储
    pub fn tasks(&self) -> &TaskRepository {
        &self.task_repo
    }

    // === Connection CRUD Operations ===

    /// 创建或更新连接配置
    pub async fn upsert_connection(&self, connection: &ConnectionConfig) -> Result<()> {
        self.connection_repo.upsert(connection).await
    }

    /// 根据TRN获取连接配置
    pub async fn get_connection(&self, trn: &str) -> Result<Option<ConnectionConfig>> {
        self.connection_repo.get_by_trn(trn).await
    }

    /// 列出连接配置
    pub async fn list_connections(
        &self,
        auth_type: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ConnectionConfig>> {
        self.connection_repo.list(auth_type, limit, offset).await
    }

    /// 删除连接配置
    pub async fn delete_connection(&self, trn: &str) -> Result<bool> {
        // 先删除相关的任务
        let deleted_tasks = self.task_repo.delete_by_connection(trn).await?;
        tracing::info!("Deleted {} tasks for connection {}", deleted_tasks, trn);

        // 再删除连接
        let ok = self.connection_repo.delete(trn).await?;
        // 失效缓存（该连接关联的所有task）
        self.invalidate_cache_by_connection(trn).await;
        Ok(ok)
    }

    /// 统计连接数量
    pub async fn count_connections(&self, auth_type: Option<&str>) -> Result<i64> {
        self.connection_repo.count_by_type(auth_type).await
    }

    // === Task CRUD Operations ===

    /// 创建或更新任务配置
    pub async fn upsert_task(&self, task: &TaskConfig) -> Result<()> {
        // 验证关联的连接是否存在
        if !self
            .task_repo
            .validate_connection_exists(&task.connection_trn)
            .await?
        {
            return Err(anyhow!("Connection not found: {}", task.connection_trn));
        }

        self.task_repo.upsert(task).await?;
        // 失效该task缓存
        self.invalidate_cache_by_task(&task.trn).await;
        Ok(())
    }

    /// 根据TRN获取任务配置
    pub async fn get_task(&self, trn: &str) -> Result<Option<TaskConfig>> {
        self.task_repo.get_by_trn(trn).await
    }

    /// 列出任务配置
    pub async fn list_tasks(
        &self,
        connection_trn: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskConfig>> {
        self.task_repo.list(connection_trn, limit, offset).await
    }

    /// 删除任务配置
    pub async fn delete_task(&self, trn: &str) -> Result<bool> {
        let ok = self.task_repo.delete(trn).await?;
        if ok {
            self.invalidate_cache_by_task(trn).await;
        }
        Ok(ok)
    }

    /// 统计任务数量
    pub async fn count_tasks(&self, connection_trn: Option<&str>) -> Result<i64> {
        self.task_repo.count_by_connection(connection_trn).await
    }

    // === Advanced Operations ===

    /// 获取完整的执行上下文（连接+任务），带TTL缓存
    pub async fn get_execution_context(
        &self,
        task_trn: &str,
    ) -> Result<Option<(ConnectionConfig, TaskConfig)>> {
        if let Some(ctx) = self
            .get_cached_execution_context(task_trn, Duration::from_secs(60))
            .await
        {
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

    /// 批量导入连接和任务
    pub async fn import_configurations(
        &self,
        connections: Vec<ConnectionConfig>,
        tasks: Vec<TaskConfig>,
    ) -> Result<(usize, usize)> {
        let mut imported_connections = 0;
        let mut imported_tasks = 0;

        // 导入连接
        for connection in connections {
            match self.upsert_connection(&connection).await {
                Ok(_) => imported_connections += 1,
                Err(e) => tracing::warn!("Failed to import connection {}: {}", connection.trn, e),
            }
        }

        // 导入任务
        for task in tasks {
            match self.upsert_task(&task).await {
                Ok(_) => imported_tasks += 1,
                Err(e) => tracing::warn!("Failed to import task {}: {}", task.trn, e),
            }
        }

        Ok((imported_connections, imported_tasks))
    }

    /// 导出所有配置
    pub async fn export_configurations(&self) -> Result<(Vec<ConnectionConfig>, Vec<TaskConfig>)> {
        let connections = self.list_connections(None, None, None).await?;
        let tasks = self.list_tasks(None, None, None).await?;
        Ok((connections, tasks))
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<()> {
        self.db_manager.health_check().await
    }

    /// 获取存储统计信息
    pub async fn get_stats(&self) -> Result<StorageStats> {
        let db_stats = self.db_manager.get_stats().await?;

        // 按认证类型统计连接
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

    /// 清理过期数据
    pub async fn cleanup(&self) -> Result<CleanupResult> {
        let expired_auth_connections = self.db_manager.cleanup_expired_auth_connections().await?;

        Ok(CleanupResult {
            expired_auth_connections,
        })
    }

    /// 按 TRN 执行
    pub async fn execute_by_trn(&self, task_trn: &str) -> Result<ExecutionResult> {
        let (conn, task) = self
            .get_execution_context(task_trn)
            .await?
            .ok_or_else(|| anyhow!("Task not found: {}", task_trn))?;
        let executor = Executor::new();
        executor.execute(&conn, &task).await
    }
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_connections: i64,
    pub total_tasks: i64,
    pub total_auth_connections: i64,
    pub api_key_connections: i64,
    pub basic_connections: i64,
    pub oauth2_cc_connections: i64,
    pub oauth2_ac_connections: i64,
}

/// 清理结果
#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub expired_auth_connections: u64,
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
            "trn:connection:test".to_string(),
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
            "trn:task:test".to_string(),
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
