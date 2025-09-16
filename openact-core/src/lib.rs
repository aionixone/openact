//! OpenAct v2 核心引擎
//! 
//! 基于 AWS EventBridge Connection + Step Functions HTTP Task 设计
//! 提供统一的 API 客户端解决方案

pub mod config;
pub mod engine;
pub mod trn;
pub mod http;
pub mod merge;
pub mod error;

// 重新导出核心类型
pub use config::{ConnectionConfig, TaskConfig, AuthParameters};
pub use engine::{TaskExecutor, ExecutionResult};
pub use trn::{OpenActTrn, TrnManager};
pub use error::{OpenActError, Result};

/// OpenAct v2 主要客户端
#[derive(Debug)]
pub struct OpenAct {
    task_executor: TaskExecutor,
    trn_manager: TrnManager,
}

impl OpenAct {
    /// 创建新的 OpenAct 实例
    pub fn new() -> Result<Self> {
        let task_executor = TaskExecutor::new()?;
        let trn_manager = TrnManager::new();
        
        Ok(Self {
            task_executor,
            trn_manager,
        })
    }

    /// 注册 Connection 配置
    pub async fn register_connection(&mut self, connection: ConnectionConfig) -> Result<()> {
        self.trn_manager.register_connection(connection).await
    }

    /// 注册 Task 配置
    pub async fn register_task(&mut self, task: TaskConfig) -> Result<()> {
        self.trn_manager.register_task(task).await
    }

    /// 执行任务
    pub async fn execute_task(&self, task_trn: &str, input: serde_json::Value) -> Result<ExecutionResult> {
        self.task_executor.execute_by_trn(task_trn, input).await
    }

    /// 列出所有 Connection
    pub async fn list_connections(&self, pattern: &str) -> Result<Vec<ConnectionConfig>> {
        self.trn_manager.list_connections(pattern).await
    }

    /// 列出所有 Task
    pub async fn list_tasks(&self, pattern: &str) -> Result<Vec<TaskConfig>> {
        self.trn_manager.list_tasks(pattern).await
    }
}

impl Default for OpenAct {
    fn default() -> Self {
        Self::new().expect("Failed to create OpenAct instance")
    }
}
