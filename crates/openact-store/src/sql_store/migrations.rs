use crate::error::StoreResult;
use sqlx::SqlitePool;

/// Database migration manager
pub struct MigrationRunner {
    pool: SqlitePool,
}

impl MigrationRunner {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations
    pub async fn migrate(&self) -> StoreResult<()> {
        // Create migrations tracking table if not exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Check what migrations are already applied
        let applied_versions: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM _migrations ORDER BY version")
                .fetch_all(&self.pool)
                .await?;

        // Run migration 001 if not applied
        if !applied_versions.contains(&1) {
            self.run_migration_001().await?;

            // Record migration as applied
            sqlx::query("INSERT INTO _migrations (version, name) VALUES (1, '001_initial_schema')")
                .execute(&self.pool)
                .await?;
        }

        // Run migration 002 if not applied
        if !applied_versions.contains(&2) {
            self.run_migration_002().await?;

            // Record migration as applied
            sqlx::query("INSERT INTO _migrations (version, name) VALUES (2, '002_add_mcp_fields')")
                .execute(&self.pool)
                .await?;
        }

        if !applied_versions.contains(&3) {
            self.run_migration_003().await?;

            sqlx::query(
                "INSERT INTO _migrations (version, name) VALUES (3, '003_orchestrator_runtime')",
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn run_migration_001(&self) -> StoreResult<()> {
        let migration_sql = include_str!("../../migrations/001_initial_schema.sql");

        // Execute migration in a transaction
        let mut tx = self.pool.begin().await?;

        // SQLite: execute multi-statement script safely (handle triggers with BEGIN..END;)
        let mut buffer = String::new();
        let mut inside_trigger = false;
        for raw_line in migration_sql.lines() {
            let line = raw_line.trim_end();

            // Accumulate lines (keep original spacing/newlines inside triggers)
            buffer.push_str(line);
            buffer.push('\n');

            let upper = line.trim_start().to_uppercase();
            if upper.starts_with("CREATE TRIGGER") {
                inside_trigger = true;
            }

            // Statement ends when we see a semicolon and we're not inside a trigger
            // or when we see END; closing a trigger
            let ends_with_semicolon = line.trim_end().ends_with(';');
            let is_end_of_trigger = inside_trigger && upper.ends_with("END;");

            if (ends_with_semicolon && !inside_trigger) || is_end_of_trigger {
                // Remove leading blank/comment lines
                let mut lines: Vec<&str> = buffer.lines().collect();
                while let Some(first) = lines.first() {
                    let t = first.trim_start();
                    if t.is_empty() || t.starts_with("--") {
                        lines.remove(0);
                    } else {
                        break;
                    }
                }
                let stmt = lines.join("\n").trim().to_string();
                if !stmt.is_empty() {
                    sqlx::query(&stmt).execute(&mut *tx).await?;
                }
                buffer.clear();
                if is_end_of_trigger {
                    inside_trigger = false;
                }
            }
        }

        // Execute any trailing statement without semicolon
        let mut trailing_lines: Vec<&str> = buffer.lines().collect();
        while let Some(first) = trailing_lines.first() {
            let t = first.trim_start();
            if t.is_empty() || t.starts_with("--") {
                trailing_lines.remove(0);
            } else {
                break;
            }
        }
        let trailing_stmt = trailing_lines.join("\n").trim().to_string();
        if !trailing_stmt.is_empty() {
            sqlx::query(&trailing_stmt).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn run_migration_002(&self) -> StoreResult<()> {
        let migration_sql = include_str!("../../migrations/002_add_mcp_fields.sql");

        // Execute migration in a transaction
        let mut tx = self.pool.begin().await?;

        // Execute each statement separately (simpler for this migration)
        for line in migration_sql.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with("--") && line.ends_with(';') {
                sqlx::query(line).execute(&mut *tx).await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    async fn run_migration_003(&self) -> StoreResult<()> {
        let migration_sql = include_str!("../../migrations/003_orchestrator_runtime.sql");
        let mut tx = self.pool.begin().await?;

        let mut buffer = String::new();
        let mut inside_trigger = false;

        for raw_line in migration_sql.lines() {
            let line = raw_line.trim_end();

            buffer.push_str(line);
            buffer.push('\n');

            let upper = line.trim_start().to_uppercase();
            if upper.starts_with("CREATE TRIGGER") {
                inside_trigger = true;
            }

            let ends_with_semicolon = line.trim_end().ends_with(';');
            let is_end_of_trigger = inside_trigger && upper.ends_with("END;");

            if (ends_with_semicolon && !inside_trigger) || is_end_of_trigger {
                let mut lines: Vec<&str> = buffer.lines().collect();
                while let Some(first) = lines.first() {
                    let trimmed = first.trim_start();
                    if trimmed.is_empty() || trimmed.starts_with("--") {
                        lines.remove(0);
                    } else {
                        break;
                    }
                }
                let statement = lines.join("\n").trim().trim_end_matches(';').trim().to_string();
                if !statement.is_empty() {
                    sqlx::query(&statement).execute(&mut *tx).await?;
                }
                buffer.clear();
                if is_end_of_trigger {
                    inside_trigger = false;
                }
            }
        }

        let mut trailing_lines: Vec<&str> = buffer.lines().collect();
        while let Some(first) = trailing_lines.first() {
            let trimmed = first.trim_start();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                trailing_lines.remove(0);
            } else {
                break;
            }
        }
        let trailing_stmt = trailing_lines.join("\n").trim().to_string();
        if !trailing_stmt.is_empty() {
            sqlx::query(&trailing_stmt).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
