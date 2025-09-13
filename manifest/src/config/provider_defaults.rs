use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::utils::error::{OpenApiToolError, Result};
use crate::utils::yaml_loader::load_yaml_file;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RetryDefaults {
    #[serde(default)] pub on_status: Vec<u16>,
    #[serde(default)] pub respect_retry_after: bool,
    #[serde(default)] pub strategy: Option<String>,
    #[serde(default)] pub base_ms: Option<u64>,
    #[serde(default)] pub max_retries: Option<u32>,
    #[serde(default)] pub jitter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProviderDefaults {
    #[serde(default, rename = "x-retry")] pub x_retry: Option<RetryDefaults>,
    #[serde(default, rename = "x-timeout-ms")] pub x_timeout_ms: Option<u64>,
    #[serde(default, rename = "x-ok-path")] pub x_ok_path: Option<Value>,
    #[serde(default, rename = "x-error-path")] pub x_error_path: Option<Value>,
    #[serde(default, rename = "x-pagination")] pub x_pagination: Option<Value>,
}

#[derive(Debug, Default, Clone)]
pub struct ProviderDefaultsRegistry {
    hostname_to_defaults: HashMap<String, ProviderDefaults>,
}

impl ProviderDefaultsRegistry {
    pub fn new() -> Self { Self { hostname_to_defaults: HashMap::new() } }

    /// Load from config/provider-defaults.yaml
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let value = load_yaml_file(&path)?;
        let obj = value.as_object().ok_or_else(|| OpenApiToolError::parse(
            format!("provider-defaults must be a mapping object: {}", path.as_ref().display())
        ))?;

        let mut registry = Self::new();
        for (hostname, v) in obj {
            let defaults: ProviderDefaults = serde_json::from_value(v.clone())
                .map_err(|e| OpenApiToolError::parse(format!("invalid provider defaults for {}: {}", hostname, e)))?;
            registry.hostname_to_defaults.insert(hostname.clone(), defaults);
        }
        Ok(registry)
    }

    pub fn get(&self, hostname: &str) -> Option<&ProviderDefaults> {
        self.hostname_to_defaults.get(hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_provider_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", r#"
api.github.com:
  x-retry:
    on_status: [429, 500, 502, 503, 504]
    strategy: exponential
    base_ms: 400
    max_retries: 3
    jitter: full
  x-timeout-ms: 15000
  x-ok-path: null
  x-error-path: $.message
"#).unwrap();

        let reg = ProviderDefaultsRegistry::load_from_file(file.path()).unwrap();
        let d = reg.get("api.github.com").expect("defaults present");
        assert_eq!(d.x_timeout_ms, Some(15000));
        assert!(d.x_retry.as_ref().unwrap().on_status.contains(&429));
    }
}


