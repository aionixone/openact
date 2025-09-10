use crate::storage::action_models::*;
use crate::utils::error::{Result, OpenApiToolError};
use sqlx::SqlitePool;

/// Action Repository - 管理 Action 的 CRUD 操作
pub struct ActionRepository {
    pool: SqlitePool,
}

impl ActionRepository {
    /// 创建新的 Repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建 Action
    pub async fn create_action(&self, request: CreateActionRequest) -> Result<Action> {
        let now = chrono::Utc::now().naive_utc();
        let is_active = request.is_active;
        
        let result = sqlx::query!(
            r#"
            INSERT INTO actions (
                trn, tenant, name, provider, openapi_spec, 
                extensions, auth_flow, metadata, is_active,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            request.trn,
            request.tenant,
            request.name,
            request.provider,
            request.openapi_spec,
            request.extensions,
            request.auth_flow,
            request.metadata,
            is_active,
            now,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to create action: {}", e)))?;

        let action_id = result.last_insert_rowid();
        self.get_action_by_id(action_id).await
    }

    /// 根据 ID 获取 Action
    pub async fn get_action_by_id(&self, id: i64) -> Result<Action> {
        let row = sqlx::query_as!(
            Action,
            "SELECT * FROM actions WHERE id = ?",
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get action by id: {}", e)))?;

        Ok(row)
    }

    /// 根据 TRN 获取 Action
    pub async fn get_action_by_trn(&self, trn: &str) -> Result<Action> {
        let row = sqlx::query_as!(
            Action,
            "SELECT * FROM actions WHERE trn = ?",
            trn
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get action by trn: {}", e)))?;

        Ok(row)
    }

    /// 根据租户获取 Actions
    pub async fn get_actions_by_tenant(&self, tenant: &str, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Action>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        
        let rows = sqlx::query_as!(
            Action,
            "SELECT * FROM actions WHERE tenant = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            tenant, limit, offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get actions by tenant: {}", e)))?;

        Ok(rows)
    }

    /// 根据提供商获取 Actions
    pub async fn get_actions_by_provider(&self, provider: &str, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Action>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);
        
        let rows = sqlx::query_as!(
            Action,
            "SELECT * FROM actions WHERE provider = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            provider, limit, offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to get actions by provider: {}", e)))?;

        Ok(rows)
    }

    /// 更新 Action
    pub async fn update_action(&self, id: i64, request: UpdateActionRequest) -> Result<Action> {
        let now = chrono::Utc::now().naive_utc();
        
        sqlx::query!(
            r#"
            UPDATE actions SET
                openapi_spec = COALESCE(?, openapi_spec),
                extensions = COALESCE(?, extensions),
                auth_flow = COALESCE(?, auth_flow),
                metadata = COALESCE(?, metadata),
                is_active = COALESCE(?, is_active),
                updated_at = ?
            WHERE id = ?
            "#,
            request.openapi_spec,
            request.extensions,
            request.auth_flow,
            request.metadata,
            request.is_active,
            now,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to update action: {}", e)))?;

        self.get_action_by_id(id).await
    }

    /// 删除 Action
    pub async fn delete_action(&self, id: i64) -> Result<()> {
        sqlx::query!(
            "DELETE FROM actions WHERE id = ?",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to delete action: {}", e)))?;

        Ok(())
    }

    /// 检查 Action 是否存在
    pub async fn action_exists(&self, trn: &str) -> Result<bool> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM actions WHERE trn = ?",
            trn
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OpenApiToolError::database(format!("Failed to check action existence: {}", e)))?;

        Ok(count > 0)
    }

    /// 获取 Action 统计信息
    pub async fn get_action_stats(&self, tenant: Option<&str>) -> Result<ActionStats> {
        if let Some(tenant) = tenant {
            let total_actions = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM actions WHERE tenant = ?",
                tenant
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to get total actions: {}", e)))?;

            let active_actions = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM actions WHERE tenant = ? AND is_active = 1",
                tenant
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to get active actions: {}", e)))?;

            Ok(ActionStats {
                total_actions: total_actions.into(),
                active_actions: active_actions.into(),
                total_executions: 0, // 需要从 execution repository 获取
                successful_executions: 0,
                failed_executions: 0,
                average_duration_ms: None,
            })
        } else {
            let total_actions = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM actions"
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to get total actions: {}", e)))?;

            let active_actions = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM actions WHERE is_active = 1"
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OpenApiToolError::database(format!("Failed to get active actions: {}", e)))?;

            Ok(ActionStats {
                total_actions: total_actions.into(),
                active_actions: active_actions.into(),
                total_executions: 0, // 需要从 execution repository 获取
                successful_executions: 0,
                failed_executions: 0,
                average_duration_ms: None,
            })
        }
    }
}