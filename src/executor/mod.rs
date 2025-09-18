//! 统一执行器模块
//! 
//! 提供统一的API调用执行器，支持所有认证类型：
//! - API Key、Basic Auth、OAuth2 Client Credentials、OAuth2 Authorization Code
//! - 自动处理token刷新、参数合并、认证注入

pub mod http_executor;
pub mod auth_injector;
pub mod parameter_merger;

#[cfg(test)]
pub mod integration_tests;

pub use http_executor::HttpExecutor;
pub use auth_injector::{AuthInjector, AuthInjectionError};
pub use parameter_merger::{ParameterMerger, MergedParameters};

use crate::models::{ConnectionConfig, TaskConfig};
use anyhow::Result;
use serde_json::Value;

/// 执行结果
#[derive(Debug)]
pub struct ExecutionResult {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Value,
}

/// 主执行器：统一处理所有认证类型的API调用
pub struct Executor {
    http_executor: HttpExecutor,
}

impl Executor {
    /// 创建新的执行器实例
    pub fn new() -> Self {
        Self {
            http_executor: HttpExecutor::new(),
        }
    }

    /// 执行API调用（支持所有认证类型，包括自动token刷新）
    pub async fn execute(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<ExecutionResult> {
        let response = self.http_executor.execute(connection, task).await?;
        
        // 提取响应信息
        let status = response.status().as_u16();
        let headers = response.headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        
        // 读取body
        let body = response.json::<Value>().await?;
        
        Ok(ExecutionResult {
            status,
            headers,
            body,
        })
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}
