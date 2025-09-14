use crate::storage::action_models::*;
use crate::utils::error::{Result, OpenApiToolError};
use sqlx::SqlitePool;

/// Execution Repository - 管理 Action 执行记录的 CRUD 操作
pub struct ExecutionRepository {
    pool: SqlitePool,
}

impl ExecutionRepository {
    /// 创建新的 Repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 确保执行表存在（用于初始化）
    pub async fn ensure_table_exists(&self) -> Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS action_executions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                execution_trn TEXT NOT NULL,
                action_trn TEXT NOT NULL,
                tenant TEXT NOT NULL,
                input_data TEXT,
                output_data TEXT,
                status TEXT NOT NULL,
                status_code INTEGER,
                error_message TEXT,
                duration_ms INTEGER,
                retry_count INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to create executions table: {}", e)))?;
        
        Ok(())
    }

    /// 创建执行记录
    pub async fn create_execution(&self, request: CreateExecutionRequest) -> Result<ActionExecution> {
        let now = chrono::Utc::now().naive_utc();
        let status = "pending".to_string();
        
        let result = sqlx::query!(
            r#"
            INSERT INTO action_executions (
                execution_trn, action_trn, tenant, input_data, 
                status, retry_count, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            request.execution_trn,
            request.action_trn,
            request.tenant,
            request.input_data,
            status,
            0i64,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to create execution: {}", e)))?;

        let execution_id = result.last_insert_rowid();
        self.get_execution_by_id(execution_id).await
    }

    /// 根据 ID 获取执行记录
    pub async fn get_execution_by_id(&self, id: i64) -> Result<ActionExecution> {
        let row = sqlx::query_as!(
            ActionExecution,
            "SELECT * FROM action_executions WHERE id = ?",
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get execution by id: {}", e)))?;

        Ok(row)
    }

    /// 根据执行 TRN 获取执行记录
    pub async fn get_execution_by_trn(&self, execution_trn: &str) -> Result<ActionExecution> {
        let row = sqlx::query_as!(
            ActionExecution,
            "SELECT * FROM action_executions WHERE execution_trn = ?",
            execution_trn
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get execution by trn: {}", e)))?;

        Ok(row)
    }

    /// 根据 Action TRN 获取执行记录
    pub async fn get_executions_by_action_trn(&self, action_trn: &str, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<ActionExecution>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        
        let rows = sqlx::query_as!(
            ActionExecution,
            "SELECT * FROM action_executions WHERE action_trn = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            action_trn, limit, offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get executions by action trn: {}", e)))?;

        Ok(rows)
    }

    /// 根据租户获取执行记录
    pub async fn get_executions_by_tenant(&self, tenant: &str, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<ActionExecution>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        
        let rows = sqlx::query_as!(
            ActionExecution,
            "SELECT * FROM action_executions WHERE tenant = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            tenant, limit, offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get executions by tenant: {}", e)))?;

        Ok(rows)
    }

    /// 更新执行状态
    pub async fn update_execution_status(&self, id: i64, status: &str, completed_at: Option<chrono::NaiveDateTime>) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE action_executions SET
                status = ?,
                completed_at = ?
            WHERE id = ?
            "#,
            status,
            completed_at,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to update execution status: {}", e)))?;

        Ok(())
    }

    /// 更新执行结果
    pub async fn update_execution_result(&self, id: i64, result: ExecutionResult) -> Result<()> {
        let completed_at = if result.status == "completed" || result.status == "failed" {
            Some(chrono::Utc::now().naive_utc())
        } else {
            None
        };
        let status_code = result.status_code.map(|c| c as i64);

        sqlx::query!(
            r#"
            UPDATE action_executions SET
                output_data = ?,
                status = ?,
                status_code = ?,
                error_message = ?,
                duration_ms = ?,
                completed_at = ?
            WHERE id = ?
            "#,
            result.output_data,
            result.status,
            status_code,
            result.error_message,
            result.duration_ms,
            completed_at,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to update execution result: {}", e)))?;

        Ok(())
    }

    /// 增加重试次数
    pub async fn increment_retry_count(&self, id: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE action_executions SET retry_count = retry_count + 1 WHERE id = ?",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to increment retry count: {}", e)))?;

        Ok(())
    }

    /// 获取执行统计信息
    pub async fn get_execution_stats(&self, action_trn: Option<&str>, tenant: Option<&str>) -> Result<ExecutionStats> {
        let (total_executions, successful_executions, failed_executions, pending_executions, average_duration) = 
            if let Some(action_trn) = action_trn {
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE action_trn = ?",
                    action_trn
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get total executions: {}", e)))?;

                let successful = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE action_trn = ? AND status = 'completed'",
                    action_trn
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get successful executions: {}", e)))?;

                let failed = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE action_trn = ? AND status = 'failed'",
                    action_trn
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get failed executions: {}", e)))?;

                let pending = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE action_trn = ? AND status = 'pending'",
                    action_trn
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get pending executions: {}", e)))?;

                let avg_duration: Option<f64> = sqlx::query_scalar!(
                    "SELECT CAST(AVG(duration_ms) AS REAL) FROM action_executions WHERE action_trn = ? AND duration_ms IS NOT NULL",
                    action_trn
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get average duration: {}", e)))?;

                (total, successful, failed, pending, avg_duration)
            } else if let Some(tenant) = tenant {
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE tenant = ?",
                    tenant
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get total executions: {}", e)))?;

                let successful = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE tenant = ? AND status = 'completed'",
                    tenant
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get successful executions: {}", e)))?;

                let failed = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE tenant = ? AND status = 'failed'",
                    tenant
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get failed executions: {}", e)))?;

                let pending = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE tenant = ? AND status = 'pending'",
                    tenant
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get pending executions: {}", e)))?;

                let avg_duration: Option<f64> = sqlx::query_scalar!(
                    "SELECT CAST(AVG(duration_ms) AS REAL) FROM action_executions WHERE tenant = ? AND duration_ms IS NOT NULL",
                    tenant
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get average duration: {}", e)))?;

                (total, successful, failed, pending, avg_duration)
            } else {
                let total = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions"
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get total executions: {}", e)))?;

                let successful = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE status = 'completed'"
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get successful executions: {}", e)))?;

                let failed = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE status = 'failed'"
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get failed executions: {}", e)))?;

                let pending = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM action_executions WHERE status = 'pending'"
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get pending executions: {}", e)))?;

                let avg_duration: Option<f64> = sqlx::query_scalar!(
                    "SELECT CAST(AVG(duration_ms) AS REAL) FROM action_executions WHERE duration_ms IS NOT NULL"
                )
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OpenApiToolError::database(format!("Failed to get average duration: {}", e)))?;

                (total, successful, failed, pending, avg_duration)
            };

        let success_rate = if total_executions > 0 {
            Some(successful_executions as f64 / total_executions as f64)
        } else {
            None
        };

        Ok(ExecutionStats {
            total_executions: total_executions.into(),
            successful_executions: successful_executions.into(),
            failed_executions: failed_executions.into(),
            pending_executions: pending_executions.into(),
            average_duration_ms: average_duration,
            success_rate,
        })
    }

    /// 清理旧的执行记录
    pub async fn cleanup_old_executions(&self, cutoff_date: chrono::NaiveDateTime) -> Result<i64> {
        let result = sqlx::query!(
            "DELETE FROM action_executions WHERE created_at < ?",
            cutoff_date
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to cleanup old executions: {}", e)))?;

        Ok(result.rows_affected() as i64)
    }
}