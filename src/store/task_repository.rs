//! Task repository for managing Task configurations
//!
//! This module provides CRUD operations for Task configurations,
//! with support for JSON serialization of complex parameters.

use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::{SqlitePool, Row};
use serde_json;

use crate::models::TaskConfig;

/// Repository for managing Task configurations
pub struct TaskRepository {
    pool: SqlitePool,
}

impl TaskRepository {
    /// Create a new TaskRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create or update a task
    pub async fn upsert(&self, task: &TaskConfig) -> Result<()> {
        // Serialize optional JSON fields
        let headers_json = task.headers
            .as_ref()
            .map(|h| serde_json::to_string(h))
            .transpose()?;

        let query_params_json = task.query_params
            .as_ref()
            .map(|q| serde_json::to_string(q))
            .transpose()?;

        let request_body_json = task.request_body
            .as_ref()
            .map(|b| serde_json::to_string(b))
            .transpose()?;

        let timeout_config_json = task.timeout_config
            .as_ref()
            .map(|tc| serde_json::to_string(tc))
            .transpose()?;

        let network_config_json = task.network_config
            .as_ref()
            .map(|nc| serde_json::to_string(nc))
            .transpose()?;

        let http_policy_json = task.http_policy
            .as_ref()
            .map(|hp| serde_json::to_string(hp))
            .transpose()?;

        let response_policy_json = task.response_policy
            .as_ref()
            .map(|rp| serde_json::to_string(rp))
            .transpose()?;

        let retry_policy_json = task.retry_policy
            .as_ref()
            .map(|rp| serde_json::to_string(rp))
            .transpose()?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO tasks (
                trn, name, connection_trn, api_endpoint, method,
                headers_json, query_params_json, request_body_json,
                timeout_config_json, network_config_json, http_policy_json, response_policy_json, retry_policy_json,
                created_at, updated_at, version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&task.trn)
        .bind(&task.name)
        .bind(&task.connection_trn)
        .bind(&task.api_endpoint)
        .bind(&task.method)
        .bind(&headers_json)
        .bind(&query_params_json)
        .bind(&request_body_json)
        .bind(&timeout_config_json)
        .bind(&network_config_json)
        .bind(&http_policy_json)
        .bind(&response_policy_json)
        .bind(&retry_policy_json)
        .bind(&task.created_at)
        .bind(&Utc::now()) // Always update updated_at
        .bind(&task.version)
        .execute(&self.pool)
        .await
        .map_err(|e| anyhow!("Failed to upsert task: {}", e))?;

        Ok(())
    }

    /// Get a task by TRN
    pub async fn get_by_trn(&self, trn: &str) -> Result<Option<TaskConfig>> {
        let row = sqlx::query(
            r#"
            SELECT trn, name, connection_trn, api_endpoint, method,
                   headers_json, query_params_json, request_body_json,
                   timeout_config_json, network_config_json, http_policy_json, response_policy_json,
                   created_at, updated_at, version
            FROM tasks WHERE trn = ?1
            "#,
        )
        .bind(trn)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch task: {}", e))?;

        match row {
            Some(row) => {
                let task = self.row_to_task_config(row)?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }

    /// List tasks with optional filtering
    pub async fn list(&self, connection_trn: Option<&str>, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<TaskConfig>> {
        let query = if let Some(conn_trn) = connection_trn {
            sqlx::query(
                r#"
                SELECT trn, name, connection_trn, api_endpoint, method,
                       headers_json, query_params_json, request_body_json,
                       timeout_config_json, network_config_json, http_policy_json, response_policy_json,
                       created_at, updated_at, version
                FROM tasks 
                WHERE connection_trn = ?1
                ORDER BY created_at DESC
                LIMIT ?2 OFFSET ?3
                "#,
            )
            .bind(conn_trn)
            .bind(limit.unwrap_or(100))
            .bind(offset.unwrap_or(0))
        } else {
            sqlx::query(
                r#"
                SELECT trn, name, connection_trn, api_endpoint, method,
                       headers_json, query_params_json, request_body_json,
                       timeout_config_json, network_config_json, http_policy_json, response_policy_json,
                       created_at, updated_at, version
                FROM tasks
                ORDER BY created_at DESC
                LIMIT ?1 OFFSET ?2
                "#,
            )
            .bind(limit.unwrap_or(100))
            .bind(offset.unwrap_or(0))
        };

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to list tasks: {}", e))?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(self.row_to_task_config(row)?);
        }

        Ok(tasks)
    }

    /// Delete a task by TRN
    pub async fn delete(&self, trn: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM tasks WHERE trn = ?1")
            .bind(trn)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to delete task: {}", e))?;

        Ok(result.rows_affected() > 0)
    }

    /// Count tasks by connection
    pub async fn count_by_connection(&self, connection_trn: Option<&str>) -> Result<i64> {
        let count = if let Some(conn_trn) = connection_trn {
            sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE connection_trn = ?1")
                .bind(conn_trn)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
                .fetch_one(&self.pool)
                .await?
        };

        Ok(count)
    }

    /// Validate that connection_trn exists
    pub async fn validate_connection_exists(&self, connection_trn: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections WHERE trn = ?1")
            .bind(connection_trn)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to validate connection: {}", e))?;

        Ok(count > 0)
    }

    /// Delete all tasks for a connection (cascade delete)
    pub async fn delete_by_connection(&self, connection_trn: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tasks WHERE connection_trn = ?1")
            .bind(connection_trn)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Failed to delete tasks by connection: {}", e))?;

        Ok(result.rows_affected())
    }

    /// Convert database row to TaskConfig
    fn row_to_task_config(&self, row: sqlx::sqlite::SqliteRow) -> Result<TaskConfig> {
        // Parse optional JSON fields
        let headers = self.parse_optional_json(&row, "headers_json")?;
        let query_params = self.parse_optional_json(&row, "query_params_json")?;
        let request_body = self.parse_optional_json(&row, "request_body_json")?;
        let timeout_config = self.parse_optional_json(&row, "timeout_config_json")?;
        let network_config = self.parse_optional_json(&row, "network_config_json")?;
        let http_policy = self.parse_optional_json(&row, "http_policy_json")?;
        let response_policy = self.parse_optional_json(&row, "response_policy_json")?;
        let retry_policy = self.parse_optional_json(&row, "retry_policy_json")?;

        Ok(TaskConfig {
            trn: row.get("trn"),
            name: row.get("name"),
            connection_trn: row.get("connection_trn"),
            api_endpoint: row.get("api_endpoint"),
            method: row.get("method"),
            headers,
            query_params,
            request_body,
            timeout_config,
            network_config,
            http_policy,
            response_policy,
            retry_policy,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            version: row.get("version"),
        })
    }

    /// Parse optional JSON field from database row
    fn parse_optional_json<T>(&self, row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let json_str: Option<String> = row.try_get(column).ok();
        match json_str {
            Some(s) if !s.is_empty() => {
                serde_json::from_str(&s)
                    .map(Some)
                    .map_err(|e| anyhow!("Failed to parse JSON field {}: {}", column, e))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::store::database::DatabaseManager;
    use std::collections::HashMap;
    use crate::models::{ConnectionConfig, AuthorizationType, ApiKeyAuthParameters};

    async fn create_test_repo() -> (TaskRepository, DatabaseManager) {
        let database_url = "sqlite::memory:";
        let manager = DatabaseManager::new(database_url).await.unwrap();
        (TaskRepository::new(manager.pool().clone()), manager)
    }

    async fn ensure_connection(manager: &DatabaseManager, trn: &str) {
        let mut conn_cfg = ConnectionConfig::new(
            trn.to_string(),
            format!("Conn for {}", trn),
            AuthorizationType::ApiKey,
        );
        conn_cfg.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "test".to_string(),
        });
        let conn_repo = manager.connection_repository();
        conn_repo.upsert(&conn_cfg).await.unwrap();
    }

    #[tokio::test]
    async fn test_task_crud_operations() {
        let (repo, manager) = create_test_repo().await;

        // Create test task
        let connection_trn = "trn:connection:test";
        ensure_connection(&manager, connection_trn).await;

        let mut task = TaskConfig::new(
            "trn:task:test-1".to_string(),
            "Test Task".to_string(),
            connection_trn.to_string(),
            "https://api.example.com/users".to_string(),
            "GET".to_string(),
        );

        // Add some headers
        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), vec!["application/json".to_string()]);
        task.headers = Some(headers);

        // Test create
        repo.upsert(&task).await.unwrap();

        // Test read
        let retrieved = repo.get_by_trn(&task.trn).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.trn, task.trn);
        assert_eq!(retrieved.name, task.name);
        assert_eq!(retrieved.api_endpoint, task.api_endpoint);

        // Test update
        task.name = "Updated Task".to_string();
        repo.upsert(&task).await.unwrap();
        let updated = repo.get_by_trn(&task.trn).await.unwrap().unwrap();
        assert_eq!(updated.name, "Updated Task");

        // Test list
        let tasks = repo.list(None, None, None).await.unwrap();
        assert_eq!(tasks.len(), 1);

        // Test count
        let count = repo.count_by_connection(None).await.unwrap();
        assert_eq!(count, 1);

        // Test delete
        let deleted = repo.delete(&task.trn).await.unwrap();
        assert!(deleted);
        
        let not_found = repo.get_by_trn(&task.trn).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_list_with_filtering() {
        let (repo, manager) = create_test_repo().await;

        // Create tasks for different connections
        ensure_connection(&manager, "trn:connection:conn1").await;
        ensure_connection(&manager, "trn:connection:conn2").await;
        let task1 = TaskConfig::new(
            "trn:task:test-1".to_string(),
            "Task 1".to_string(),
            "trn:connection:conn1".to_string(),
            "https://api.example.com/users".to_string(),
            "GET".to_string(),
        );
        
        let task2 = TaskConfig::new(
            "trn:task:test-2".to_string(),
            "Task 2".to_string(),
            "trn:connection:conn2".to_string(),
            "https://api.example.com/posts".to_string(),
            "POST".to_string(),
        );

        repo.upsert(&task1).await.unwrap();
        repo.upsert(&task2).await.unwrap();

        // Test filtering by connection
        let conn1_tasks = repo.list(Some("trn:connection:conn1"), None, None).await.unwrap();
        assert_eq!(conn1_tasks.len(), 1);
        assert_eq!(conn1_tasks[0].trn, "trn:task:test-1");

        // Test pagination
        let limited_tasks = repo.list(None, Some(1), Some(0)).await.unwrap();
        assert_eq!(limited_tasks.len(), 1);
    }
}
