use serde_json::Value;
use std::path::Path;

use crate::utils::error::Result;
use super::provider_auth_defaults::ProviderAuthDefaultsRegistry;
use super::provider_defaults::ProviderDefaultsRegistry;
use super::sidecar_overrides::SidecarOverridesRegistry;
use super::merger::{build_merged_config};

#[derive(Debug)]
pub struct ConfigRegistry {
    pub auth_defaults: ProviderAuthDefaultsRegistry,
    pub provider_defaults: ProviderDefaultsRegistry,
    pub sidecar_overrides: SidecarOverridesRegistry,
}

impl ConfigRegistry {
    pub fn empty() -> Self {
        Self {
            auth_defaults: super::provider_auth_defaults::ProviderAuthDefaultsRegistry::new(),
            provider_defaults: super::provider_defaults::ProviderDefaultsRegistry::new(),
            sidecar_overrides: super::sidecar_overrides::SidecarOverridesRegistry::new(),
        }
    }

    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        let auth_path = dir.join("provider-auth-defaults.yaml");
        let defaults_path = dir.join("provider-defaults.yaml");
        let sidecar_path = dir.join("sidecar-overrides.yaml");

        Ok(Self {
            auth_defaults: ProviderAuthDefaultsRegistry::load_from_file(auth_path)?,
            provider_defaults: ProviderDefaultsRegistry::load_from_file(defaults_path)?,
            sidecar_overrides: SidecarOverridesRegistry::load_from_file(sidecar_path)?,
        })
    }

    /// Build merged x-* config for an operation
    /// - provider_host: e.g., "api.github.com"
    /// - operation_id: e.g., "github.user.get"
    /// - action_extensions: the operation-level extensions object (serde_json::Value::Object)
    pub fn merged_for(&self, provider_host: &str, operation_id: &str, action_extensions: &Value) -> Value {
        let pad = self
            .auth_defaults
            .get(provider_host)
            .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        let pd = self
            .provider_defaults
            .get(provider_host)
            .map(|d| serde_json::to_value(d).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        let sidecar = self
            .sidecar_overrides
            .get(operation_id)
            .cloned()
            .unwrap_or(Value::Null);

        build_merged_config(&pad, &pd, action_extensions, &sidecar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use tempfile::tempdir;
    use serde_json::json;
    use std::fs;

    #[test]
    fn test_registry_merge_from_dir() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();

        fs::write(
            dir.join("provider-auth-defaults.yaml"),
            r#"api.github.com:
  scheme: oauth2
  injection:
    type: jsonada
    mapping: |
      {% {\"headers\": {\"Authorization\": \"Bearer \" & $access_token } } %}
"#,
        )
        .unwrap();

        fs::write(
            dir.join("provider-defaults.yaml"),
            r#"api.github.com:
  x-timeout-ms: 10000
  x-retry:
    max_retries: 1
"#,
        )
        .unwrap();

        fs::write(
            dir.join("sidecar-overrides.yaml"),
            r#"github.user.get:
  x-retry:
    max_retries: 5
"#,
        )
        .unwrap();

        let reg = ConfigRegistry::load_from_dir(dir).unwrap();
        let action_ext = json!({"x-timeout-ms": 20000});
        let merged = reg.merged_for("api.github.com", "github.user.get", &action_ext);
        assert_eq!(merged["x-timeout-ms"], 20000);
        assert_eq!(merged["x-retry"]["max_retries"], 5);
        assert_eq!(merged["scheme"], "oauth2");
    }
}


