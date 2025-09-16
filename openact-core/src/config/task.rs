//! Task 配置管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

use crate::error::{OpenActError, Result};
use super::types::*;

/// AWS Step Functions HTTP Task 兼容的 Task 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskConfig {
    /// TRN 标识符
    #[serde(rename = "trn")]
    pub trn: String,
    
    /// Task 名称
    pub name: String,
    
    /// 任务类型（固定为 Http）
    #[serde(rename = "Type")]
    pub task_type: String,
    
    /// 引用的 Connection TRN
    pub resource: String,
    
    /// HTTP 请求参数
    pub parameters: TaskParameters,
    
    /// 重试配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
    
    /// 超时设置（秒）- 兼容性保留
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    
    /// 细化超时配置（优先级高于 timeout_seconds）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeouts: Option<crate::config::types::TimeoutConfig>,
    
    /// 网络配置（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<crate::config::types::NetworkConfig>,
    
    /// Transform 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<TransformConfig>,
    
    /// Safety 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety: Option<SafetyConfig>,
    
    /// 创建时间
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    
    /// 更新时间
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

/// Task 参数配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskParameters {
    /// API 端点 - 支持静态值或 JSONata 表达式
    pub api_endpoint: crate::config::types::Mapping,
    
    /// HTTP 方法 - 支持静态值或 JSONata 表达式
    pub method: crate::config::types::Mapping,
    
    /// Headers (Task 级别) - 支持多值和 JSONata 表达式
    #[serde(default)]
    pub headers: HashMap<String, crate::config::types::MultiValue>,
    
    /// Query Parameters (Task 级别) - 支持多值和 JSONata 表达式
    #[serde(default)]
    pub query_parameters: HashMap<String, crate::config::types::MultiValue>,
    
    /// Request Body - 支持含 JSONata 表达式的 JSON 结构
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<serde_json::Value>,
}

impl TaskConfig {
    /// 创建新的 Task 配置
    pub fn new(
        trn: String,
        name: String,
        resource: String,
        parameters: TaskParameters,
    ) -> Self {
        let now = Utc::now();
        Self {
            trn,
            name,
            task_type: "Http".to_string(),
            resource,
            parameters,
            retry: None,
            timeout_seconds: None,
            timeouts: None,
            network: None,
            transform: None,
            safety: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        // 验证 TRN 格式
        if !self.trn.starts_with("trn:openact:") {
            return Err(OpenActError::task_config("Invalid TRN format"));
        }

        // 验证 Resource TRN 格式
        if !self.resource.starts_with("trn:openact:") {
            return Err(OpenActError::task_config("Invalid Resource TRN format"));
        }

        // 验证 Task 类型
        if self.task_type != "Http" {
            return Err(OpenActError::task_config("Only Http task type is supported"));
        }

        // 验证 API 端点（允许动态表达式，此处仅检查非空）
        if self.parameters.api_endpoint.is_empty() {
            return Err(OpenActError::task_config("API endpoint is required"));
        }

        // 验证 HTTP 方法（允许动态表达式，此处只校验静态场景）
        match self.parameters.method.to_uppercase().as_str() {
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => {}
            _ => return Err(OpenActError::task_config("Invalid HTTP method")),
        }

        Ok(())
    }

    /// 获取连接 TRN
    pub fn connection_trn(&self) -> &str {
        &self.resource
    }

    /// 获取 HTTP 参数
    pub fn get_http_parameters(&self) -> TaskHttpParameters {
        TaskHttpParameters {
            headers: self.parameters.headers.clone(),
            query_parameters: self.parameters.query_parameters.clone(),
            request_body: self.parameters.request_body.clone(),
        }
    }

    /// 设置重试配置
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = Some(retry);
        self
    }

    /// 设置超时
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    /// 设置 Transform 配置
    pub fn with_transform(mut self, transform: TransformConfig) -> Self {
        self.transform = Some(transform);
        self
    }

    /// 设置 Safety 配置
    pub fn with_safety(mut self, safety: SafetyConfig) -> Self {
        self.safety = Some(safety);
        self
    }
}

/// Task 配置存储
#[derive(Debug)]
pub struct TaskConfigStore {
    tasks: HashMap<String, TaskConfig>,
}

impl TaskConfigStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// 存储 Task 配置
    pub async fn store(&mut self, task: TaskConfig) -> Result<()> {
        task.validate()?;
        self.tasks.insert(task.trn.clone(), task);
        Ok(())
    }

    /// 获取 Task 配置
    pub async fn get(&self, trn: &str) -> Result<Option<TaskConfig>> {
        Ok(self.tasks.get(trn).cloned())
    }

    /// 列出匹配模式的 Task
    pub async fn list(&self, pattern: &str) -> Result<Vec<TaskConfig>> {
        let mut results = Vec::new();
        
        for task in self.tasks.values() {
            if pattern == "*" || task.trn.contains(pattern) {
                results.push(task.clone());
            }
        }
        
        Ok(results)
    }

    /// 删除 Task 配置
    pub async fn delete(&mut self, trn: &str) -> Result<bool> {
        Ok(self.tasks.remove(trn).is_some())
    }

    /// 根据 Connection TRN 查找相关 Task
    pub async fn find_by_connection(&self, connection_trn: &str) -> Result<Vec<TaskConfig>> {
        let mut results = Vec::new();
        
        for task in self.tasks.values() {
            if task.resource == connection_trn {
                results.push(task.clone());
            }
        }
        
        Ok(results)
    }
}

impl Default for TaskConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_config_basic() {
        let parameters = TaskParameters {
            api_endpoint: "https://api.github.com/user/repos".to_string().into(),
            method: "GET".to_string().into(),
            headers: HashMap::new(),
            query_parameters: HashMap::new(),
            request_body: None,
        };

        let task = TaskConfig::new(
            "trn:openact:tenant1:task/list-repos@v1".to_string(),
            "List GitHub Repositories".to_string(),
            "trn:openact:tenant1:connection/github@v1".to_string(),
            parameters,
        );

        assert!(task.validate().is_ok());
        assert_eq!(task.connection_trn(), "trn:openact:tenant1:connection/github@v1");
        assert_eq!(task.task_type, "Http");
    }

    #[tokio::test]
    async fn test_task_config_with_options() {
        let parameters = TaskParameters {
            api_endpoint: "https://api.stripe.com/v1/invoices".to_string().into(),
            method: "POST".to_string().into(),
            headers: {
                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), "application/json".to_string().into());
                headers
            },
            query_parameters: HashMap::new(),
            request_body: Some(serde_json::json!({
                "customer": "cus_123",
                "description": "Monthly subscription"
            })),
        };

        let retry = RetryConfig {
            max_attempts: 3,
            backoff_rate: 2.0,
            interval_seconds: 1,
            retry_on_status: vec![429, 503, 504],
            retry_on_errors: vec!["timeout".to_string(), "io".to_string(), "tls".to_string()],
            jitter_strategy: JitterStrategy::Full,
            respect_retry_after: true,
        };

        let task = TaskConfig::new(
            "trn:openact:tenant1:task/create-invoice@v1".to_string(),
            "Create Stripe Invoice".to_string(),
            "trn:openact:tenant1:connection/stripe@v1".to_string(),
            parameters,
        )
        .with_retry(retry)
        .with_timeout(60)
        .with_transform(TransformConfig {
            request_body_encoding: RequestBodyEncoding::None,
            request_encoding_options: None,
        })
        .with_safety(SafetyConfig {
            idempotency: true,
        });

        assert!(task.validate().is_ok());
        assert!(task.retry.is_some());
        assert_eq!(task.timeout_seconds, Some(60));
        assert!(task.transform.is_some());
        assert!(task.safety.is_some());
    }

    #[tokio::test]
    async fn test_task_store() {
        let mut store = TaskConfigStore::new();
        
        let parameters = TaskParameters {
            api_endpoint: "https://api.test.com/data".to_string().into(),
            method: "GET".to_string().into(),
            headers: HashMap::new(),
            query_parameters: HashMap::new(),
            request_body: None,
        };

        let task = TaskConfig::new(
            "trn:openact:tenant1:task/get-data@v1".to_string(),
            "Get Data".to_string(),
            "trn:openact:tenant1:connection/test@v1".to_string(),
            parameters,
        );

        // 存储
        store.store(task.clone()).await.unwrap();

        // 获取
        let retrieved = store.get(&task.trn).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Get Data");

        // 根据 Connection 查找
        let found = store.find_by_connection("trn:openact:tenant1:connection/test@v1").await.unwrap();
        assert_eq!(found.len(), 1);
    }
}
