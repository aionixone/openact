//! 数据库管理器
//! 
//! 提供统一的数据库连接池管理和初始化逻辑

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::env;
use std::path::Path;
use tokio::fs;

use crate::store::encryption::FieldEncryption;
use super::connection_repository::ConnectionRepository;

/// 数据库管理器
pub struct DatabaseManager {
    pool: SqlitePool,
    encryption: Option<FieldEncryption>,
}

impl DatabaseManager {
    /// 从环境变量创建数据库管理器
    pub async fn from_env() -> Result<Self> {
        let database_url = env::var("OPENACT_DB_URL")
            .unwrap_or_else(|_| "./data/openact.db".to_string());
        
        Self::new(&database_url).await
    }

    /// 创建新的数据库管理器
    pub async fn new(database_url: &str) -> Result<Self> {
        // 确保数据库目录存在
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

        // 规范化数据库URL，确保具有写权限
        let normalized_url = if database_url.starts_with("sqlite:") {
            // 如果已经有sqlite前缀，检查是否有mode参数
            if database_url.contains("mode=") {
                database_url.to_string()
            } else {
                let separator = if database_url.contains("?") { "&" } else { "?" };
                format!("{}{}mode=rwc", database_url, separator)
            }
        } else {
            // 使用 URL 形式，确保为 sqlite:// 路径，避免某些平台的权限问题
            format!("sqlite://{}?mode=rwc", database_url)
        };

        // 创建连接池
        let pool = SqlitePool::connect(&normalized_url)
            .await
            .context("Failed to connect to database")?;

        // 初始化加密服务
        let encryption = match env::var("OPENACT_MASTER_KEY") {
            Ok(_) => Some(FieldEncryption::from_env()?),
            Err(_) => {
                tracing::warn!("OPENACT_MASTER_KEY not set, sensitive fields will not be encrypted");
                None
            }
        };

        let manager = Self { pool, encryption };
        
        // 初始化数据库schema
        manager.initialize_schema().await?;
        
        Ok(manager)
    }

    /// 获取连接池
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// 获取加密服务
    pub fn encryption(&self) -> &Option<FieldEncryption> {
        &self.encryption
    }

    /// 创建 ConnectionRepository
    pub fn connection_repository(&self) -> ConnectionRepository {
        ConnectionRepository::new(self.pool.clone(), self.encryption.clone())
    }

    /// 初始化数据库schema - 使用迁移系统
    async fn initialize_schema(&self) -> Result<()> {
        tracing::info!("Running database migrations...");
        
        // 运行所有待执行的迁移
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("Failed to run database migrations")?;

        tracing::info!("Database migrations completed successfully");
        Ok(())
    }


    /// 健康检查
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .context("Database health check failed")?;
        Ok(())
    }

    /// 获取数据库统计信息
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let auth_connections_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_connections")
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

    /// 清理过期的认证连接
    pub async fn cleanup_expired_auth_connections(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM auth_connections WHERE expires_at < datetime('now')")
            .execute(&self.pool)
            .await
            .context("Failed to cleanup expired auth connections")?;

        Ok(result.rows_affected())
    }
}

/// 数据库统计信息
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
        
        // 健康检查
        manager.health_check().await.unwrap();
        
        // 获取统计信息
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

        // 测试repository创建成功（通过健康检查验证）
        manager.health_check().await.unwrap();
    }
}
