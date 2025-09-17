use anyhow::Result;

pub async fn run_shared_migrations() -> Result<()> {
    let cfg = openact_storage::config::DatabaseConfig::from_env();
    let pool = openact_storage::pool::get_pool(&cfg).await?;
    openact_storage::migrate::run(&pool).await?;
    Ok(())
}
