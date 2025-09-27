//! Unified configuration management for OpenAct
use std::env;

/// Main configuration structure for OpenAct
#[derive(Debug, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub encryption: EncryptionConfig,
    pub client_pool: ClientPoolConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct EncryptionConfig {
    pub master_key: Option<String>,
    pub key_rotation_enabled: bool,
    pub current_key_version: u32,
}

#[derive(Debug, Clone)]
pub struct ClientPoolConfig {
    pub capacity: usize,
    pub ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub port: u16,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            database: DatabaseConfig::from_env(),
            encryption: EncryptionConfig::from_env(),
            client_pool: ClientPoolConfig::from_env(),
            server: ServerConfig::from_env(),
        }
    }
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        let url = env::var("OPENACT_DB_URL")
            .unwrap_or_else(|_| "sqlite:./data/openact.db?mode=rwc".to_string());

        Self { url }
    }
}

impl EncryptionConfig {
    pub fn from_env() -> Self {
        Self {
            master_key: env::var("OPENACT_MASTER_KEY").ok(),
            key_rotation_enabled: env::var("OPENACT_KEY_ROTATION").unwrap_or_default() == "true",
            current_key_version: env::var("OPENACT_KEY_VERSION")
                .unwrap_or_else(|_| "1".to_string())
                .parse()
                .unwrap_or(1),
        }
    }
}

impl ClientPoolConfig {
    pub fn from_env() -> Self {
        let capacity = env::var("OPENACT_CLIENT_POOL_CAPACITY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|c| *c > 0)
            .unwrap_or(16);

        let ttl_secs = env::var("OPENACT_CLIENT_POOL_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);

        Self { capacity, ttl_secs }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let bind_addr = env::var("OPENACT_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = env::var("OPENACT_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8080);

        Self { bind_addr, port }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_config_from_env() {
        // Set test env vars
        unsafe {
            env::set_var("OPENACT_DB_URL", "sqlite:test.db");
            env::set_var("OPENACT_MASTER_KEY", "deadbeef");
            env::set_var("OPENACT_CLIENT_POOL_CAPACITY", "32");
            env::set_var("OPENACT_PORT", "9090");
        }

        let config = Config::from_env();

        assert_eq!(config.database.url, "sqlite:test.db");
        assert_eq!(config.encryption.master_key, Some("deadbeef".to_string()));
        assert_eq!(config.client_pool.capacity, 32);
        assert_eq!(config.server.port, 9090);

        // Clean up
        unsafe {
            env::remove_var("OPENACT_DB_URL");
            env::remove_var("OPENACT_MASTER_KEY");
            env::remove_var("OPENACT_CLIENT_POOL_CAPACITY");
            env::remove_var("OPENACT_PORT");
        }
    }
}
