use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, NaiveDateTime};
use sqlx::FromRow;

/// Action 定义模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Action {
    pub id: Option<i64>,
    pub trn: String,
    pub tenant: String,
    pub name: String,
    pub provider: String,
    pub openapi_spec: String,  // JSON 字符串
    pub extensions: Option<String>,  // JSON 字符串
    pub auth_flow: Option<String>,   // JSON 字符串
    pub metadata: Option<String>,    // JSON 字符串
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Action 执行记录模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActionExecution {
    pub id: Option<i64>,
    pub execution_trn: String,
    pub action_trn: String,
    pub tenant: String,
    pub input_data: Option<String>,  // JSON 字符串
    pub output_data: Option<String>, // JSON 字符串
    pub status: String,
    pub status_code: Option<i64>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub retry_count: i64,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
}

/// Action 测试用例模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActionTest {
    pub id: Option<i64>,
    pub action_trn: String,
    pub test_name: String,
    pub input_data: String,  // JSON 字符串
    pub expected_output: Option<String>,  // JSON 字符串
    pub test_type: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Action 测试结果模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActionTestResult {
    pub id: Option<i64>,
    pub test_id: i64,
    pub execution_id: Option<i64>,
    pub status: String,
    pub actual_output: Option<String>,  // JSON 字符串
    pub diff_data: Option<String>,      // JSON 字符串
    pub duration_ms: Option<i64>,
    pub created_at: NaiveDateTime,
}

/// Action 性能指标模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActionMetric {
    pub id: Option<i64>,
    pub action_trn: String,
    pub tenant: String,
    pub metric_type: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub timestamp: NaiveDateTime,
}

/// Action 配置模型
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActionConfiguration {
    pub id: Option<i64>,
    pub action_trn: String,
    pub config_key: String,
    pub config_value: String,
    pub config_type: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

// ===== 请求和响应类型 =====

/// 创建 Action 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActionRequest {
    pub trn: String,
    pub tenant: String,
    pub name: String,
    pub provider: String,
    pub openapi_spec: String,
    pub extensions: Option<String>,
    pub auth_flow: Option<String>,
    pub metadata: Option<String>,
    pub is_active: bool,
}

/// 更新 Action 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateActionRequest {
    pub openapi_spec: Option<String>,
    pub extensions: Option<String>,
    pub auth_flow: Option<String>,
    pub metadata: Option<String>,
    pub is_active: bool,
}

/// 创建执行记录请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExecutionRequest {
    pub execution_trn: String,
    pub action_trn: String,
    pub tenant: String,
    pub input_data: Option<String>,
}

/// 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub output_data: Option<String>,
    pub status: String,
    pub status_code: Option<i32>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Action 搜索查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSearchQuery {
    pub tenant: Option<String>,
    pub provider: Option<String>,
    pub name_pattern: Option<String>,
    pub is_active: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 执行记录搜索查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSearchQuery {
    pub action_trn: Option<String>,
    pub tenant: Option<String>,
    pub status: Option<String>,
    pub created_after: Option<NaiveDateTime>,
    pub created_before: Option<NaiveDateTime>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Action 统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStats {
    pub total_actions: i64,
    pub active_actions: i64,
    pub total_executions: i64,
    pub successful_executions: i64,
    pub failed_executions: i64,
    pub average_duration_ms: Option<f64>,
}

/// 执行记录统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStats {
    pub total_executions: i64,
    pub successful_executions: i64,
    pub failed_executions: i64,
    pub pending_executions: i64,
    pub average_duration_ms: Option<f64>,
    pub success_rate: Option<f64>,
}

// ===== 辅助方法 =====

impl Action {
    /// 转换为 UTC 时间戳
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }

    /// 转换为 UTC 时间戳
    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.updated_at, Utc)
    }

    /// 检查是否激活
    pub fn is_active(&self) -> bool {
        self.is_active
    }
}

impl ActionExecution {
    /// 转换为 UTC 时间戳
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }

    /// 转换为 UTC 时间戳
    pub fn completed_at_utc(&self) -> Option<DateTime<Utc>> {
        self.completed_at.map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
    }

    /// 检查是否成功
    pub fn is_successful(&self) -> bool {
        self.status == "completed" && self.status_code.map_or(false, |code| code >= 200 && code < 300)
    }

    /// 检查是否失败
    pub fn is_failed(&self) -> bool {
        self.status == "failed" || self.status_code.map_or(false, |code| code >= 400)
    }
}

impl ActionTest {
    /// 转换为 UTC 时间戳
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }

    /// 转换为 UTC 时间戳
    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.updated_at, Utc)
    }

    /// 检查是否激活
    pub fn is_active(&self) -> bool {
        self.is_active
    }
}

impl ActionTestResult {
    /// 转换为 UTC 时间戳
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }
}

impl ActionMetric {
    /// 转换为 UTC 时间戳
    pub fn timestamp_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.timestamp, Utc)
    }
}

impl ActionConfiguration {
    /// 转换为 UTC 时间戳
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.created_at, Utc)
    }

    /// 转换为 UTC 时间戳
    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self.updated_at, Utc)
    }

    /// 检查是否激活
    pub fn is_active(&self) -> bool {
        self.is_active
    }
}