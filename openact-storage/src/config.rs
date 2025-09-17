use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub dsn: String,
    pub max_connections: u32,
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        let dsn = std::env::var("OPENACT_DB_URL").unwrap_or_else(|_| {
            // Prefer workspace root ./data/openact.db when detectable
            let resolved = resolve_workspace_root()
                .map(|root| format!("sqlite:{}/data/openact.db", root.display()))
                .unwrap_or_else(|| "sqlite:./data/openact.db".to_string());
            resolved
        });
        let max_connections = std::env::var("OPENACT_DB_MAX_CONN")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(10);
        Self { dsn, max_connections }
    }
}

fn resolve_workspace_root() -> Option<std::path::PathBuf> {
    // Allow explicit override
    if let Ok(root) = std::env::var("OPENACT_WORKSPACE_ROOT") {
        let p = std::path::PathBuf::from(root);
        if p.is_dir() { return Some(p); }
    }

    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        let cargo_lock = dir.join("Cargo.lock");
        let git_dir = dir.join(".git");
        if cargo_lock.exists() || git_dir.exists() || cargo_toml.exists() {
            return Some(dir);
        }
        if !(dir.pop()) { break; }
    }
    None
}
