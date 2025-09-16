//! 执行结果类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// 任务执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// 执行状态
    pub status: ExecutionStatus,
    
    /// HTTP 状态码
    pub status_code: Option<u16>,
    
    /// 响应头
    pub headers: HashMap<String, String>,
    
    /// 响应体
    pub body: serde_json::Value,
    
    /// 执行时间统计
    pub timing: ExecutionTiming,
    
    /// 重试信息
    pub retry_info: Option<RetryInfo>,
    
    /// 错误信息
    pub error: Option<String>,
}

/// 执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    /// 成功
    Success,
    /// 失败
    Failed,
    /// 超时
    Timeout,
    /// 取消
    Cancelled,
}

/// 执行时间统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTiming {
    /// 开始时间
    pub started_at: DateTime<Utc>,
    
    /// 结束时间
    pub finished_at: DateTime<Utc>,
    
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
    
    /// 连接耗时（毫秒）
    pub connect_duration_ms: Option<u64>,
    
    /// 请求耗时（毫秒）
    pub request_duration_ms: Option<u64>,
}

/// 重试信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryInfo {
    /// 重试次数
    pub attempt_count: u32,
    
    /// 最大重试次数
    pub max_attempts: u32,
    
    /// 重试原因
    pub retry_reasons: Vec<String>,
    
    /// 总重试耗时（毫秒）
    pub total_retry_duration_ms: u64,
}

impl ExecutionResult {
    /// 创建成功结果
    pub fn success(
        status_code: u16,
        headers: HashMap<String, String>,
        body: serde_json::Value,
        timing: ExecutionTiming,
    ) -> Self {
        Self {
            status: ExecutionStatus::Success,
            status_code: Some(status_code),
            headers,
            body,
            timing,
            retry_info: None,
            error: None,
        }
    }

    /// 创建失败结果
    pub fn failed(
        error: String,
        timing: ExecutionTiming,
        status_code: Option<u16>,
        retry_info: Option<RetryInfo>,
    ) -> Self {
        Self {
            status: ExecutionStatus::Failed,
            status_code,
            headers: HashMap::new(),
            body: serde_json::Value::Null,
            timing,
            retry_info,
            error: Some(error),
        }
    }

    /// 创建超时结果
    pub fn timeout(timing: ExecutionTiming, retry_info: Option<RetryInfo>) -> Self {
        Self {
            status: ExecutionStatus::Timeout,
            status_code: None,
            headers: HashMap::new(),
            body: serde_json::Value::Null,
            timing,
            retry_info,
            error: Some("Request timed out".to_string()),
        }
    }

    /// 检查是否成功
    pub fn is_success(&self) -> bool {
        self.status == ExecutionStatus::Success
    }

    /// 检查是否失败
    pub fn is_failed(&self) -> bool {
        !self.is_success()
    }

    /// 获取响应体为字符串
    pub fn body_as_string(&self) -> Option<String> {
        match &self.body {
            serde_json::Value::String(s) => Some(s.clone()),
            _ => serde_json::to_string(&self.body).ok(),
        }
    }

    /// 获取总耗时
    pub fn total_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.timing.total_duration_ms)
    }
}

impl ExecutionTiming {
    /// 创建新的时间统计
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            finished_at: started_at, // 临时设置，会在完成时更新
            total_duration_ms: 0,
            connect_duration_ms: None,
            request_duration_ms: None,
        }
    }

    /// 标记完成并计算耗时
    pub fn finish(&mut self) {
        self.finished_at = Utc::now();
        self.total_duration_ms = (self.finished_at - self.started_at)
            .num_milliseconds() as u64;
    }

    /// 设置连接耗时
    pub fn set_connect_duration(&mut self, duration: std::time::Duration) {
        self.connect_duration_ms = Some(duration.as_millis() as u64);
    }

    /// 设置请求耗时
    pub fn set_request_duration(&mut self, duration: std::time::Duration) {
        self.request_duration_ms = Some(duration.as_millis() as u64);
    }
}

impl RetryInfo {
    /// 创建新的重试信息
    pub fn new(max_attempts: u32) -> Self {
        Self {
            attempt_count: 0,
            max_attempts,
            retry_reasons: Vec::new(),
            total_retry_duration_ms: 0,
        }
    }

    /// 增加重试次数
    pub fn increment_attempt(&mut self, reason: String) {
        self.attempt_count += 1;
        self.retry_reasons.push(reason);
    }

    /// 添加重试耗时
    pub fn add_retry_duration(&mut self, duration: std::time::Duration) {
        self.total_retry_duration_ms += duration.as_millis() as u64;
    }

    /// 检查是否还能重试
    pub fn can_retry(&self) -> bool {
        self.attempt_count < self.max_attempts
    }
}
