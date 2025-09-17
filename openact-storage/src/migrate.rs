use crate::error::Result;
use crate::pool::DbPool;

pub async fn run(pool: &DbPool) -> Result<()> {
    sqlx::migrate!("migrations/sqlite")
        .run(pool)
        .await
        .map_err(|e| anyhow::Error::new(e))?;
    Ok(())
}
