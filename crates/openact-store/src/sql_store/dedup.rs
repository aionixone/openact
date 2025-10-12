use aionix_contracts::idempotency::DedupStore;
use log::warn;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct SqliteDedupStore {
    pool: SqlitePool,
}

impl SqliteDedupStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DedupStore for SqliteDedupStore {
    fn check_and_record(&self, key: &str) -> bool {
        let pool = self.pool.clone();
        let key = key.to_string();

        let fut = async move {
            let res = sqlx::query(
                r#"
                INSERT INTO outbox_dedup_keys (key, created_at)
                VALUES (?1, CURRENT_TIMESTAMP)
                ON CONFLICT(key) DO NOTHING
                "#,
            )
            .bind(&key)
            .execute(&pool)
            .await;

            match res {
                Ok(result) => Ok(result.rows_affected() == 0),
                Err(err) => {
                    if let Some(db_err) = err.as_database_error() {
                        if db_err.message().contains("UNIQUE") {
                            return Ok(true);
                        }
                    }
                    warn!("dedup store insert failed: {err}");
                    Err(err)
                }
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut).unwrap_or(true)),
            Err(_) => tokio::runtime::Runtime::new()
                .map(|rt| rt.block_on(fut).unwrap_or(true))
                .unwrap_or(true),
        }
    }

    fn remove(&self, key: &str) {
        let pool = self.pool.clone();
        let key = key.to_string();
        let fut = async move {
            if let Err(err) = sqlx::query("DELETE FROM outbox_dedup_keys WHERE key = ?1")
                .bind(&key)
                .execute(&pool)
                .await
            {
                warn!("dedup store remove failed: {err}");
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
            Err(_) => {
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    let _ = rt.block_on(fut);
                }
            }
        }
    }
}

impl From<&SqlitePool> for SqliteDedupStore {
    fn from(pool: &SqlitePool) -> Self {
        Self::new(pool.clone())
    }
}

pub fn create_sqlite_dedup_store(pool: &SqlitePool) -> Arc<dyn DedupStore> {
    Arc::new(SqliteDedupStore::from(pool))
}
