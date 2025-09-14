use crate::database::CoreDatabase;
use crate::error::Result;

/// Core configuration loaded from environment
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub database_url: String,
    pub tenant: String,
}

impl CoreConfig {
    pub fn from_env() -> Self {
        let database_url = std::env::var("OPENACT_DATABASE_URL")
            .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
            .unwrap_or_else(|_| "sqlite:./data/openact.db".to_string());
        let tenant = std::env::var("OPENACT_TENANT").unwrap_or_else(|_| "default".to_string());
        Self { database_url, tenant }
    }
}

/// Core runtime context (DB + tenant)
#[derive(Clone)]
pub struct CoreContext {
    pub db: CoreDatabase,
    pub tenant: String,
}

impl CoreContext {
    pub async fn initialize(cfg: &CoreConfig) -> Result<Self> {
        let db = CoreDatabase::connect(&cfg.database_url).await?;
        db.migrate_bindings().await?;
        Ok(Self { db, tenant: cfg.tenant.clone() })
    }

    /// Aggregate stats from DB
    pub async fn stats(&self) -> Result<crate::database::CoreStats> {
        self.db.stats().await
    }

    /// Health check
    pub async fn health(&self) -> Result<()> { self.db.health_check().await }
}


