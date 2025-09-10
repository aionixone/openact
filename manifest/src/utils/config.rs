use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::{Result, Context};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub trn: TrnConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub enable_logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
    pub request_timeout_secs: u64,
    pub max_request_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrnConfig {
    pub default_platform: String,
    pub default_scope: String,
    pub default_tag: String,
    pub version_strategy: String, // "auto", "manual", "timestamp"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encryption_key: String,
    pub jwt_secret: String,
    pub session_timeout_hours: u64,
    pub max_upload_size_mb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: "sqlite:openapi_tools.db".to_string(),
                max_connections: 10,
                enable_logging: false,
            },
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                cors_origins: vec!["*".to_string()],
                request_timeout_secs: 30,
                max_request_size: 10 * 1024 * 1024, // 10MB
            },
            trn: TrnConfig {
                default_platform: "user".to_string(),
                default_scope: "default".to_string(),
                default_tag: "prod".to_string(),
                version_strategy: "auto".to_string(),
            },
            security: SecurityConfig {
                encryption_key: "change-me-in-production".to_string(),
                jwt_secret: "change-me-in-production".to_string(),
                session_timeout_hours: 24,
                max_upload_size_mb: 50,
            },
        }
    }
}

impl Config {
    /// Load configuration from file, falling back to defaults
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            tracing::info!("Config file {:?} not found, using defaults", path);
            return Ok(Self::default());
        }
        
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: Config = if path.extension().and_then(|s| s.to_str()) == Some("yaml") 
            || path.extension().and_then(|s| s.to_str()) == Some("yml") {
            serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse YAML config: {:?}", path))?
        } else {
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML config: {:?}", path))?
        };
        
        tracing::info!("Loaded configuration from: {:?}", path);
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;
        
        tracing::info!("Saved configuration to: {:?}", path);
        Ok(())
    }
    
    /// Create a sample configuration file
    pub fn create_sample<P: AsRef<Path>>(path: P) -> Result<()> {
        let config = Self::default();
        config.save(path)
    }
} 