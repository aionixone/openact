use crate::error::{CoreError, Result};
use manifest::storage::{
    action_models::{CreateActionRequest, UpdateActionRequest},
    action_repository::ActionRepository,
};
// use serde::Deserialize; // no longer needed after enforcing OpenAPI-only
use sqlx::SqlitePool;
use std::path::Path;

#[derive(Clone)]
pub struct ActionRegistry {
    pool: SqlitePool,
}

impl ActionRegistry {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn ensure_actions_table(&self) -> Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS actions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trn TEXT UNIQUE NOT NULL,
                tenant TEXT NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                openapi_spec TEXT NOT NULL,
                extensions TEXT,
                auth_flow TEXT,
                metadata TEXT,
                is_active BOOLEAN DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(())
    }

    /// Register action from a YAML file with minimal schema validation
    pub async fn register_from_yaml(
        &self,
        tenant: &str,
        provider: &str,
        name: &str,
        trn: &str,
        yaml_path: &Path,
    ) -> Result<manifest::storage::action_models::Action> {
        self.ensure_actions_table().await?;
        let content = std::fs::read_to_string(yaml_path)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        // Enforce OpenAPI 3.0: must parse as OpenApi30Spec
        let _spec: manifest::spec::api_spec::OpenApi30Spec = serde_yaml::from_str(&content)
            .map_err(|e| CoreError::InvalidInput(format!(
                "invalid OpenAPI (expected 3.0): {}",
                e
            )))?;
        let req = CreateActionRequest {
            trn: trn.to_string(),
            tenant: tenant.to_string(),
            name: name.to_string(),
            provider: provider.to_string(),
            openapi_spec: content, // store raw content
            extensions: None,
            auth_flow: None,
            metadata: None,
            is_active: true,
        };
        let repo = ActionRepository::new(self.pool.clone());
        let action = repo
            .create_action(req)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(action)
    }

    pub async fn get_by_trn(&self, trn: &str) -> Result<manifest::storage::action_models::Action> {
        self.ensure_actions_table().await?;
        let repo = ActionRepository::new(self.pool.clone());
        repo.get_action_by_trn(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    pub async fn list_by_tenant(
        &self,
        tenant: &str,
    ) -> Result<Vec<manifest::storage::action_models::Action>> {
        self.ensure_actions_table().await?;
        let repo = ActionRepository::new(self.pool.clone());
        repo.get_actions_by_tenant(tenant, Some(100), Some(0))
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    pub async fn delete_by_trn(&self, trn: &str) -> Result<bool> {
        self.ensure_actions_table().await?;
        let repo = ActionRepository::new(self.pool.clone());
        match repo.get_action_by_trn(trn).await {
            Ok(a) => {
                let id =
                    a.id.ok_or_else(|| CoreError::InvalidInput("action id missing".into()))?;
                repo.delete_action(id)
                    .await
                    .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    pub async fn update_from_yaml(&self, trn: &str, yaml_path: &Path) -> Result<manifest::storage::action_models::Action> {
        self.ensure_actions_table().await?;
        let repo = ActionRepository::new(self.pool.clone());
        let existing = repo
            .get_action_by_trn(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        let id = existing
            .id
            .ok_or_else(|| CoreError::InvalidInput("action id missing".into()))?;
        let content = std::fs::read_to_string(yaml_path)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        let req = UpdateActionRequest {
            openapi_spec: Some(content),
            extensions: None,
            auth_flow: None,
            metadata: None,
            is_active: existing.is_active,
        };
        let updated = repo
            .update_action(id, req)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(updated)
    }

    pub async fn export_spec_by_trn(&self, trn: &str) -> Result<String> {
        self.ensure_actions_table().await?;
        let repo = ActionRepository::new(self.pool.clone());
        let a = repo
            .get_action_by_trn(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(a.openapi_spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
    use std::{io::Write, str::FromStr};

    async fn setup_memory_pool() -> SqlitePool {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        // create minimal actions table as expected by repository
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS actions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trn TEXT UNIQUE NOT NULL,
                tenant TEXT NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                openapi_spec TEXT NOT NULL,
                extensions TEXT,
                auth_flow TEXT,
                metadata TEXT,
                is_active BOOLEAN DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn register_and_get_action() {
        let pool = setup_memory_pool().await;
        let registry = ActionRegistry::new(pool);
        // create temp OpenAPI yaml
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            f,
            "{}",
            r#"openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
servers:
  - url: https://api.github.com
paths:
  /user:
    get:
      operationId: get-user
      responses:
        '200':
          description: OK
"#
        )
        .unwrap();
        let path = f.into_temp_path();
        let trn = "trn:openact:tenant1:action/github/getUser@v1";

        let created = registry
            .register_from_yaml("tenant1", "github", "getUser", trn, path.as_ref())
            .await
            .unwrap();
        assert_eq!(created.trn, trn);

        let fetched = registry.get_by_trn(trn).await.unwrap();
        assert_eq!(fetched.name, "getUser");
    }

    #[tokio::test]
    async fn register_invalid_yaml_should_fail() {
        let pool = setup_memory_pool().await;
        let registry = ActionRegistry::new(pool);
        let mut f = tempfile::NamedTempFile::new().unwrap();
        // invalid (not an OpenAPI 3.0 doc)
        writeln!(f, "name: \nmethod: GET").unwrap();
        let path = f.into_temp_path();
        let trn = "trn:openact:tenant1:action/github/getUser@v1";
        let err = registry
            .register_from_yaml("tenant1", "github", "getUser", trn, path.as_ref())
            .await
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("invalid action yaml"));
    }
}
