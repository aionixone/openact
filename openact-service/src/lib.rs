use anyhow::Result;
use openact_core::engine::executor::TaskExecutor;
use openact_registry::{Registry, WideTask, WideConnection};
use openact_storage::{config::DatabaseConfig, pool::get_pool};
pub mod oauth;

pub struct App {
    pub registry: Registry,
    pub executor: TaskExecutor,
}

impl App {
    pub async fn init() -> Result<Self> {
        let cfg = DatabaseConfig::from_env();
        let pool = get_pool(&cfg).await?;
        openact_storage::migrate::run(&pool).await?;
        let registry = Registry::from_env().await?;
        let executor = TaskExecutor::new()?;
        Ok(Self { registry, executor })
    }

    // Facade: execute
    pub async fn execute_by_trn(&self, task_trn: &str, input: serde_json::Value) -> Result<openact_core::engine::result::ExecutionResult> {
        self.executor.execute_by_trn(task_trn, input).await.map_err(Into::into)
    }

    // Facade: connections/tasks CRUD (using current Wide DTOs)
    pub async fn upsert_connection_wide(&self, conn: &WideConnection) -> Result<()> { self.registry.upsert_connection_wide(conn).await.map_err(Into::into) }
    pub async fn upsert_task_wide(&self, task: &WideTask) -> Result<()> { self.registry.upsert_task_wide(task).await.map_err(Into::into) }
    pub async fn get_task_wide(&self, trn: &str) -> Result<Option<WideTask>> { self.registry.get_task_wide(trn).await.map_err(Into::into) }
}
