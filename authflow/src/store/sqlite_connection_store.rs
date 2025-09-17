//! DB connection store adapter delegating to openact-storage

use anyhow::Result;
use async_trait::async_trait;

use crate::store::{ConnectionStore, Connection, AuthConnectionTrn};
use openact_storage::{config::DatabaseConfig, migrate, pool, repos::AuthConnectionRepository as SharedAuthRepo, models::AuthConnection as SharedAuthConnection, encryption::FieldEncryption as SharedFieldEncryption};

pub struct DbConnectionStore {
    repo: SharedAuthRepo,
}

impl DbConnectionStore {
    pub async fn new() -> Result<Self> {
        let cfg = DatabaseConfig::from_env();
        let db = pool::get_pool(&cfg).await?;
        migrate::run(&db).await?;
        let enc = SharedFieldEncryption::from_env().ok();
        let repo = SharedAuthRepo::new(db, enc);
        Ok(Self { repo })
    }
}

#[async_trait]
impl ConnectionStore for DbConnectionStore {
    async fn get(&self, connection_ref: &str) -> Result<Option<Connection>> {
        if let Some(ac) = self.repo.get_by_trn(connection_ref).await? {
            let trn = AuthConnectionTrn::new(ac.tenant, ac.provider, ac.user_id)?;
            return Ok(Some(Connection {
                trn,
                access_token: ac.access_token,
                refresh_token: ac.refresh_token,
                expires_at: ac.expires_at,
                token_type: ac.token_type,
                scope: ac.scope,
                extra: ac.extra,
                created_at: ac.created_at,
                updated_at: ac.updated_at,
            }));
        }
        Ok(None)
    }

    async fn put(&self, connection_ref: &str, connection: &Connection) -> Result<()> {
        let ac = SharedAuthConnection {
            tenant: connection.trn.tenant.clone(),
            provider: connection.trn.provider.clone(),
            user_id: connection.trn.user_id.clone(),
            access_token: connection.access_token.clone(),
            refresh_token: connection.refresh_token.clone(),
            expires_at: connection.expires_at,
            token_type: connection.token_type.clone(),
            scope: connection.scope.clone(),
            extra: connection.extra.clone(),
            created_at: connection.created_at,
            updated_at: connection.updated_at,
        };
        self.repo.upsert(connection_ref, &ac).await.map_err(|e| anyhow::anyhow!(e))
    }

    async fn delete(&self, connection_ref: &str) -> Result<bool> {
        self.repo.delete(connection_ref).await.map_err(|e| anyhow::anyhow!(e))
    }

    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&Connection>,
        new_value: Option<&Connection>,
    ) -> Result<bool> {
        let current = self.get(connection_ref).await?;
        let matches = match (expected, &current) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };
        if !matches { return Ok(false); }
        match new_value {
            Some(new_conn) => self.put(connection_ref, new_conn).await.map(|_| true),
            None => self.delete(connection_ref).await,
        }
    }

    async fn list_refs(&self) -> Result<Vec<String>> { self.repo.list_refs().await.map_err(|e| anyhow::anyhow!(e)) }

    async fn cleanup_expired(&self) -> Result<usize> { self.repo.cleanup_expired().await.map_err(|e| anyhow::anyhow!(e)) }

    async fn count(&self) -> Result<usize> { self.repo.count().await.map_err(|e| anyhow::anyhow!(e)) }
}
