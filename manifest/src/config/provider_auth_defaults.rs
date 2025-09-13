use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::utils::error::{OpenApiToolError, Result};
use crate::utils::yaml_loader::load_yaml_file;
use crate::action::{InjectionConfig, ExpiryConfig, RefreshConfig, FailureConfig};

/// Provider-level auth defaults keyed by hostname (e.g., api.github.com)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderAuthTemplate {
    #[serde(default)]
    pub scheme: Option<String>,
    pub injection: InjectionConfig,
    #[serde(default)]
    pub expiry: Option<ExpiryConfig>,
    #[serde(default)]
    pub refresh: Option<RefreshConfig>,
    #[serde(default)]
    pub failure: Option<FailureConfig>,
}

#[derive(Debug, Default, Clone)]
pub struct ProviderAuthDefaultsRegistry {
    hostname_to_template: HashMap<String, ProviderAuthTemplate>,
}

impl ProviderAuthDefaultsRegistry {
    pub fn new() -> Self { Self { hostname_to_template: HashMap::new() } }

    /// Load from config/provider-auth-defaults.yaml
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let value = load_yaml_file(&path)?;
        let obj = value.as_object().ok_or_else(|| OpenApiToolError::parse(
            format!("provider-auth-defaults must be a mapping object: {}", path.as_ref().display())
        ))?;

        let mut registry = Self::new();
        for (hostname, v) in obj {
            // Deserialize each hostname entry into ProviderAuthTemplate via JSON round-trip
            let tpl: ProviderAuthTemplate = serde_json::from_value(v.clone())
                .map_err(|e| OpenApiToolError::parse(format!("invalid provider auth defaults for {}: {}", hostname, e)))?;
            registry.hostname_to_template.insert(hostname.clone(), tpl);
        }
        Ok(registry)
    }

    pub fn get(&self, hostname: &str) -> Option<&ProviderAuthTemplate> {
        self.hostname_to_template.get(hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_provider_auth_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", r#"
api.github.com:
  scheme: oauth2
  injection:
    type: jsonada
    mapping: |
      {% {\"headers\": {\"Authorization\": \"Bearer \" & $access_token } } %}
  expiry:
    source: field
    field: $expires_at
    clock_skew_ms: 30000
  refresh:
    when: proactive_or_401
    max_retries: 1
"#).unwrap();

        let reg = ProviderAuthDefaultsRegistry::load_from_file(file.path()).unwrap();
        let tpl = reg.get("api.github.com").expect("template present");
        assert_eq!(tpl.scheme.as_deref(), Some("oauth2"));
        assert_eq!(tpl.injection.r#type, "jsonada");
    }
}


