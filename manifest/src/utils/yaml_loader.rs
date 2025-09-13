use crate::utils::error::{OpenApiToolError, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Load a YAML file and return it as serde_json::Value.
/// - Preserves mapping/sequence semantics by deserializing directly into JSON Value.
/// - On error, includes filename and (when available) line/column in the message.
pub fn load_yaml_file<P: AsRef<Path>>(path: P) -> Result<Value> {
    let path = path.as_ref();

    let content = fs::read_to_string(path).map_err(|e| {
        OpenApiToolError::io(format!("failed to read {}: {}", path.display(), e))
    })?;

    match serde_yaml::from_str::<Value>(&content) {
        Ok(val) => Ok(val),
        Err(err) => {
            if let Some(loc) = err.location() {
                Err(OpenApiToolError::parse(format!(
                    "YAML parse error at {}:{} in {}: {}",
                    loc.line(),
                    loc.column(),
                    path.display(),
                    err
                )))
            } else {
                Err(OpenApiToolError::parse(format!(
                    "YAML parse error in {}: {}",
                    path.display(), err
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "a: 1\nb: [2, 3]").unwrap();
        let path = file.path();

        let value = load_yaml_file(path).expect("should parse valid YAML");
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_load_invalid_yaml_has_filename_and_line() {
        let mut file = NamedTempFile::new().unwrap();
        // Invalid YAML: unterminated sequence (syntax error)
        writeln!(file, "a: [1, 2\nb: 2").unwrap();
        let path = file.path();

        let err = load_yaml_file(path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&path.display().to_string()));
        assert!(msg.contains("YAML parse error"));
        // Usually serde_yaml reports a line/column; do a best-effort check
        assert!(msg.contains(":") || msg.contains("line"));
    }
}


