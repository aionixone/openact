//! TRN 管理器 - 管理 Connection 和 Task 配置

use std::collections::HashMap;
use crate::config::{ConnectionConfig, TaskConfig};
use crate::error::{OpenActError, Result};
use super::parser::TrnParser;

/// TRN 资源管理器
#[derive(Debug)]
pub struct TrnManager {
    connections: HashMap<String, ConnectionConfig>,
    tasks: HashMap<String, TaskConfig>,
}

impl TrnManager {
    /// 创建新的 TRN 管理器
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            tasks: HashMap::new(),
        }
    }

    /// 注册 Connection 配置
    pub async fn register_connection(&mut self, connection: ConnectionConfig) -> Result<()> {
        // 验证 TRN 格式
        let trn = TrnParser::parse(&connection.trn)?;
        if !trn.is_connection() {
            return Err(OpenActError::trn("TRN must be a connection type"));
        }

        // 验证配置
        connection.validate()?;

        // 存储
        self.connections.insert(connection.trn.clone(), connection);
        Ok(())
    }

    /// 同步注册 Connection（无异步操作，便于启动期加载文件）
    pub fn register_connection_sync(&mut self, connection: ConnectionConfig) -> Result<()> {
        // 验证 TRN 格式
        let trn = TrnParser::parse(&connection.trn)?;
        if !trn.is_connection() {
            return Err(OpenActError::trn("TRN must be a connection type"));
        }
        // 验证配置
        connection.validate()?;
        self.connections.insert(connection.trn.clone(), connection);
        Ok(())
    }

    /// 注册 Task 配置
    pub async fn register_task(&mut self, task: TaskConfig) -> Result<()> {
        // 验证 TRN 格式
        let trn = TrnParser::parse(&task.trn)?;
        if !trn.is_task() {
            return Err(OpenActError::trn("TRN must be a task type"));
        }

        // 验证配置
        task.validate()?;

        // 验证引用的 Connection 是否存在
        if !self.connections.contains_key(&task.resource) {
            return Err(OpenActError::task_config(
                format!("Referenced connection '{}' not found", task.resource)
            ));
        }

        // 存储
        self.tasks.insert(task.trn.clone(), task);
        Ok(())
    }

    /// 同步注册 Task（便于后续需要时使用；当前未用于加载）
    pub fn register_task_sync(&mut self, task: TaskConfig) -> Result<()> {
        let trn = TrnParser::parse(&task.trn)?;
        if !trn.is_task() {
            return Err(OpenActError::trn("TRN must be a task type"));
        }
        task.validate()?;
        if !self.connections.contains_key(&task.resource) {
            return Err(OpenActError::task_config(
                format!("Referenced connection '{}' not found", task.resource)
            ));
        }
        self.tasks.insert(task.trn.clone(), task);
        Ok(())
    }

    /// 获取 Connection 配置
    pub async fn get_connection(&self, trn: &str) -> Result<Option<&ConnectionConfig>> {
        Ok(self.connections.get(trn))
    }

    /// 获取 Task 配置
    pub async fn get_task(&self, trn: &str) -> Result<Option<&TaskConfig>> {
        Ok(self.tasks.get(trn))
    }

    /// 列出匹配模式的 Connection
    pub async fn list_connections(&self, pattern: &str) -> Result<Vec<ConnectionConfig>> {
        let mut results = Vec::new();
        
        for connection in self.connections.values() {
            if TrnParser::matches_pattern(&connection.trn, pattern) {
                results.push(connection.clone());
            }
        }
        
        // 按 TRN 排序
        results.sort_by(|a, b| a.trn.cmp(&b.trn));
        Ok(results)
    }

    /// 列出匹配模式的 Task
    pub async fn list_tasks(&self, pattern: &str) -> Result<Vec<TaskConfig>> {
        let mut results = Vec::new();
        
        for task in self.tasks.values() {
            if TrnParser::matches_pattern(&task.trn, pattern) {
                results.push(task.clone());
            }
        }
        
        // 按 TRN 排序
        results.sort_by(|a, b| a.trn.cmp(&b.trn));
        Ok(results)
    }

    /// 根据 Connection TRN 查找相关 Task
    pub async fn find_tasks_by_connection(&self, connection_trn: &str) -> Result<Vec<TaskConfig>> {
        let mut results = Vec::new();
        
        for task in self.tasks.values() {
            if task.resource == connection_trn {
                results.push(task.clone());
            }
        }
        
        Ok(results)
    }

    /// 删除 Connection（会检查依赖）
    pub async fn delete_connection(&mut self, trn: &str) -> Result<bool> {
        // 检查是否有 Task 依赖此 Connection
        let dependent_tasks = self.find_tasks_by_connection(trn).await?;
        if !dependent_tasks.is_empty() {
            return Err(OpenActError::connection_config(
                format!("Cannot delete connection '{}': {} tasks depend on it", 
                    trn, dependent_tasks.len())
            ));
        }

        Ok(self.connections.remove(trn).is_some())
    }

    /// 删除 Task
    pub async fn delete_task(&mut self, trn: &str) -> Result<bool> {
        Ok(self.tasks.remove(trn).is_some())
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> TrnStats {
        TrnStats {
            connection_count: self.connections.len(),
            task_count: self.tasks.len(),
        }
    }

    /// 验证所有配置的一致性
    pub fn validate_all(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // 检查孤立的 Task（引用不存在的 Connection）
        for task in self.tasks.values() {
            if !self.connections.contains_key(&task.resource) {
                warnings.push(format!(
                    "Task '{}' references non-existent connection '{}'",
                    task.trn, task.resource
                ));
            }
        }

        // 检查未使用的 Connection
        for connection in self.connections.values() {
            let has_dependents = self.tasks.values()
                .any(|task| task.resource == connection.trn);
            
            if !has_dependents {
                warnings.push(format!(
                    "Connection '{}' is not used by any task",
                    connection.trn
                ));
            }
        }

        Ok(warnings)
    }
}

/// TRN 管理器统计信息
#[derive(Debug, Clone)]
pub struct TrnStats {
    pub connection_count: usize,
    pub task_count: usize,
}

impl Default for TrnManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::*;

    fn create_test_connection() -> ConnectionConfig {
        let auth_params = AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "X-API-Key".to_string(),
                api_key_value: crate::config::types::Credential::InlineEncrypted("test_key".to_string()),
            }),
            o_auth_parameters: None,
            basic_auth_parameters: None,
            invocation_http_parameters: None,
        };

        ConnectionConfig::new(
            "trn:openact:tenant1:connection/test@v1".to_string(),
            "Test API".to_string(),
            AuthorizationType::ApiKey,
            auth_params,
        )
    }

    fn create_test_task(connection_trn: &str) -> TaskConfig {
        let parameters = crate::config::task::TaskParameters {
            api_endpoint: "https://api.test.com/data".to_string().into(),
            method: "GET".to_string().into(),
            headers: std::collections::HashMap::new(),
            query_parameters: std::collections::HashMap::new(),
            request_body: None,
        };

        TaskConfig::new(
            "trn:openact:tenant1:task/get-data@v1".to_string(),
            "Get Data".to_string(),
            connection_trn.to_string(),
            parameters,
        )
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let mut manager = TrnManager::new();
        let connection = create_test_connection();
        let task = create_test_task(&connection.trn);

        // 注册 Connection
        manager.register_connection(connection.clone()).await.unwrap();

        // 注册 Task
        manager.register_task(task.clone()).await.unwrap();

        // 获取 Connection
        let retrieved_conn = manager.get_connection(&connection.trn).await.unwrap();
        assert!(retrieved_conn.is_some());
        assert_eq!(retrieved_conn.unwrap().name, "Test API");

        // 获取 Task
        let retrieved_task = manager.get_task(&task.trn).await.unwrap();
        assert!(retrieved_task.is_some());
        assert_eq!(retrieved_task.unwrap().name, "Get Data");
    }

    #[tokio::test]
    async fn test_list_with_patterns() {
        let mut manager = TrnManager::new();
        let connection = create_test_connection();
        manager.register_connection(connection).await.unwrap();

        // 列出所有
        let all = manager.list_connections("*").await.unwrap();
        assert_eq!(all.len(), 1);

        // 匹配模式
        let matched = manager.list_connections("*test*").await.unwrap();
        assert_eq!(matched.len(), 1);

        // 不匹配
        let not_matched = manager.list_connections("*github*").await.unwrap();
        assert_eq!(not_matched.len(), 0);
    }

    #[tokio::test]
    async fn test_dependency_validation() {
        let mut manager = TrnManager::new();
        let connection = create_test_connection();
        let task = create_test_task(&connection.trn);

        // 注册 Connection
        manager.register_connection(connection.clone()).await.unwrap();
        
        // 注册 Task
        manager.register_task(task).await.unwrap();

        // 尝试删除有依赖的 Connection（应该失败）
        let result = manager.delete_connection(&connection.trn).await;
        assert!(result.is_err());

        // 删除 Task 后再删除 Connection（应该成功）
        let task_deleted = manager.delete_task("trn:openact:tenant1:task/get-data@v1").await.unwrap();
        assert!(task_deleted);

        let connection_deleted = manager.delete_connection(&connection.trn).await.unwrap();
        assert!(connection_deleted);
    }

    #[tokio::test]
    async fn test_validation_warnings() {
        let mut manager = TrnManager::new();
        
        // 添加一个 Connection
        let connection = create_test_connection();
        manager.register_connection(connection).await.unwrap();

        // 验证（应该有未使用 Connection 的警告）
        let warnings = manager.validate_all().unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not used by any task"));
    }
}
