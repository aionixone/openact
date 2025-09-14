use crate::error::{CoreError, Result};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Binding {
    pub id: Option<i64>,
    pub tenant: String,
    pub auth_trn: String,
    pub action_trn: String,
    pub created_by: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

pub struct BindingManager {
    pool: SqlitePool,
}

impl BindingManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn bind(
        &self,
        tenant: &str,
        auth_trn: &str,
        action_trn: &str,
        created_by: Option<&str>,
    ) -> Result<Binding> {
        if tenant.is_empty() || auth_trn.is_empty() || action_trn.is_empty() {
            return Err(CoreError::InvalidInput(
                "tenant/auth_trn/action_trn required".into(),
            ));
        }

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO bindings (tenant, auth_trn, action_trn, created_by)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(tenant)
        .bind(auth_trn)
        .bind(action_trn)
        .bind(created_by)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, Binding>(
            r#"SELECT id, tenant, auth_trn, action_trn, created_by, created_at, updated_at
               FROM bindings WHERE tenant = ? AND auth_trn = ? AND action_trn = ?"#,
        )
        .bind(tenant)
        .bind(auth_trn)
        .bind(action_trn)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_by_tenant(&self, tenant: &str) -> Result<Vec<Binding>> {
        let rows = sqlx::query_as::<_, Binding>(
            r#"SELECT id, tenant, auth_trn, action_trn, created_by, created_at, updated_at
               FROM bindings WHERE tenant = ? ORDER BY created_at DESC"#,
        )
        .bind(tenant)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn unbind(&self, tenant: &str, auth_trn: &str, action_trn: &str) -> Result<bool> {
        let res = sqlx::query(
            "DELETE FROM bindings WHERE tenant = ? AND auth_trn = ? AND action_trn = ?",
        )
        .bind(tenant)
        .bind(auth_trn)
        .bind(action_trn)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Find a binding by tenant and action_trn, returning the auth_trn if present
    pub async fn get_auth_trn_for_action(
        &self,
        tenant: &str,
        action_trn: &str,
    ) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"SELECT auth_trn FROM bindings WHERE tenant = ? AND action_trn = ? LIMIT 1"#,
        )
        .bind(tenant)
        .bind(action_trn)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CoreDatabase;

    #[tokio::test]
    async fn test_bind_list_unbind() {
        let db = CoreDatabase::connect("sqlite::memory:").await.unwrap();
        db.migrate_bindings().await.unwrap();
        let mgr = BindingManager::new(db.pool().clone());

        // bind
        let b = mgr
            .bind(
                "tenant1",
                "trn:authflow:tenant1:connection/github-mock",
                "trn:openact:tenant1:action/github/getUser@v1",
                Some("tester"),
            )
            .await
            .unwrap();
        assert_eq!(b.tenant, "tenant1");

        // list
        let rows = mgr.list_by_tenant("tenant1").await.unwrap();
        assert_eq!(rows.len(), 1);

        // get auth by action
        let auth_trn = mgr
            .get_auth_trn_for_action("tenant1", "trn:openact:tenant1:action/github/getUser@v1")
            .await
            .unwrap();
        assert!(auth_trn.is_some());

        // unbind
        let ok = mgr
            .unbind(
                "tenant1",
                "trn:authflow:tenant1:connection/github-mock",
                "trn:openact:tenant1:action/github/getUser@v1",
            )
            .await
            .unwrap();
        assert!(ok);
    }
}
