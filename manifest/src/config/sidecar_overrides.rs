use std::collections::HashMap;
use std::path::Path;
use serde_json::Value;
use crate::utils::error::{OpenApiToolError, Result};
use crate::utils::yaml_loader::load_yaml_file;

/// Sidecar overrides keyed by operationId; values are arbitrary x-* objects
#[derive(Debug, Default, Clone)]
pub struct SidecarOverridesRegistry {
    operation_overrides: HashMap<String, Value>,
}

impl SidecarOverridesRegistry {
    pub fn new() -> Self { Self { operation_overrides: HashMap::new() } }

    /// Load from config/sidecar-overrides.yaml (optional file)
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Ok(Self::new());
        }
        let value = load_yaml_file(&path)?;
        let obj = value.as_object().ok_or_else(|| OpenApiToolError::parse(
            format!("sidecar-overrides must be a mapping object: {}", path_ref.display())
        ))?;

        let mut reg = Self::new();
        for (op_id, v) in obj {
            reg.operation_overrides.insert(op_id.clone(), v.clone());
        }
        Ok(reg)
    }

    pub fn get(&self, operation_id: &str) -> Option<&Value> {
        self.operation_overrides.get(operation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_sidecar_overrides() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", r#"
github.repos.list:
  x-retry:
    max_retries: 5
  x-timeout-ms: 30000
"#).unwrap();

        let reg = SidecarOverridesRegistry::load_from_file(file.path()).unwrap();
        let v = reg.get("github.repos.list").expect("override present");
        assert_eq!(v["x-timeout-ms"], 30000);
    }
}


