// Snapshot testing utilities
// Provides utilities for creating and managing test snapshots

use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::utils::error::{OpenApiToolError, Result};
use chrono::{DateTime, Utc};

/// Snapshot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Directory to store snapshots
    pub snapshot_dir: PathBuf,
    /// Whether to update snapshots automatically
    pub auto_update: bool,
    /// Snapshot format
    pub format: SnapshotFormat,
    /// Fields to exclude from snapshots
    pub exclude_fields: Vec<String>,
}

/// Snapshot format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnapshotFormat {
    Json,
    Yaml,
    PrettyJson,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            snapshot_dir: PathBuf::from("testdata/snapshots"),
            auto_update: false,
            format: SnapshotFormat::PrettyJson,
            exclude_fields: vec![
                "timestamp".to_string(),
                "execution_trn".to_string(),
                "access_token".to_string(),
                "refresh_token".to_string(),
                "id".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
            ],
        }
    }
}

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Snapshot creation time
    pub created_at: DateTime<Utc>,
    /// Snapshot version
    pub version: String,
    /// Test environment
    pub environment: HashMap<String, String>,
    /// Snapshot description
    pub description: Option<String>,
}

/// Snapshot manager
pub struct SnapshotManager {
    config: SnapshotConfig,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new(config: SnapshotConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(SnapshotConfig::default())
    }

    /// Create a snapshot
    pub fn create_snapshot(&self, name: &str, data: &Value, description: Option<&str>) -> Result<SnapshotMetadata> {
        let snapshot_path = self.get_snapshot_path(name);
        let filtered_data = self.filter_data(data)?;
        
        // Create metadata
        let metadata = SnapshotMetadata {
            created_at: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: self.get_environment_info(),
            description: description.map(|s| s.to_string()),
        };
        
        // Save snapshot
        self.save_snapshot(&snapshot_path, &filtered_data, &metadata)?;
        
        Ok(metadata)
    }

    /// Load a snapshot
    pub fn load_snapshot(&self, name: &str) -> Result<(Value, SnapshotMetadata)> {
        let snapshot_path = self.get_snapshot_path(name);
        let content = fs::read_to_string(&snapshot_path)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to read snapshot {}: {}", snapshot_path.display(), e)
            ))?;
        
        let snapshot_data: SnapshotData = serde_json::from_str(&content)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to parse snapshot {}: {}", snapshot_path.display(), e)
            ))?;
        
        Ok((snapshot_data.data, snapshot_data.metadata))
    }

    /// Compare with snapshot
    pub fn compare_with_snapshot(&self, name: &str, data: &Value) -> Result<SnapshotComparison> {
        match self.load_snapshot(name) {
            Ok((snapshot_data, metadata)) => {
                let filtered_data = self.filter_data(data)?;
                let differences = self.compare_values(&snapshot_data, &filtered_data)?;
                
                Ok(SnapshotComparison {
                    name: name.to_string(),
                    matches: differences.is_empty(),
                    differences,
                    snapshot_metadata: Some(metadata),
                })
            }
            Err(_) => {
                // Snapshot doesn't exist
                Ok(SnapshotComparison {
                    name: name.to_string(),
                    matches: false,
                    differences: vec![],
                    snapshot_metadata: None,
                })
            }
        }
    }

    /// Update a snapshot
    pub fn update_snapshot(&self, name: &str, data: &Value, description: Option<&str>) -> Result<SnapshotMetadata> {
        self.create_snapshot(name, data, description)
    }

    /// List all snapshots
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let mut snapshots = Vec::new();
        
        if !self.config.snapshot_dir.exists() {
            return Ok(snapshots);
        }
        
        let entries = fs::read_dir(&self.config.snapshot_dir)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to read snapshot directory: {}", e)
            ))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to read directory entry: {}", e)
            ))?;
            
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok((_, metadata)) = self.load_snapshot(name) {
                        snapshots.push(SnapshotInfo {
                            name: name.to_string(),
                            path: path.clone(),
                            metadata,
                        });
                    }
                }
            }
        }
        
        Ok(snapshots)
    }

    /// Delete a snapshot
    pub fn delete_snapshot(&self, name: &str) -> Result<()> {
        let snapshot_path = self.get_snapshot_path(name);
        fs::remove_file(&snapshot_path)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to delete snapshot {}: {}", snapshot_path.display(), e)
            ))
    }

    /// Get snapshot path
    fn get_snapshot_path(&self, name: &str) -> PathBuf {
        self.config.snapshot_dir.join(format!("{}.json", name))
    }

    /// Filter data by removing excluded fields
    fn filter_data(&self, data: &Value) -> Result<Value> {
        let mut filtered = data.clone();
        self.filter_value_recursive(&mut filtered, "")?;
        Ok(filtered)
    }

    /// Recursively filter values
    fn filter_value_recursive(&self, value: &mut Value, path: &str) -> Result<()> {
        match value {
            Value::Object(obj) => {
                let keys_to_remove: Vec<String> = obj.keys()
                    .filter(|key| {
                        let field_path = if path.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        self.config.exclude_fields.iter().any(|field| field_path.ends_with(field))
                    })
                    .cloned()
                    .collect();
                
                for key in keys_to_remove {
                    obj.remove(&key);
                }
                
                for (key, val) in obj.iter_mut() {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    self.filter_value_recursive(val, &field_path)?;
                }
            }
            Value::Array(arr) => {
                for (i, item) in arr.iter_mut().enumerate() {
                    let item_path = format!("{}[{}]", path, i);
                    self.filter_value_recursive(item, &item_path)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Compare two values
    fn compare_values(&self, expected: &Value, actual: &Value) -> Result<Vec<SnapshotDifference>> {
        let mut differences = Vec::new();
        self.compare_values_recursive(expected, actual, "", &mut differences)?;
        Ok(differences)
    }

    /// Recursively compare values
    fn compare_values_recursive(
        &self,
        expected: &Value,
        actual: &Value,
        path: &str,
        differences: &mut Vec<SnapshotDifference>,
    ) -> Result<()> {
        match (expected, actual) {
            (Value::Object(expected_obj), Value::Object(actual_obj)) => {
                for (key, expected_value) in expected_obj {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    
                    if let Some(actual_value) = actual_obj.get(key) {
                        self.compare_values_recursive(expected_value, actual_value, &field_path, differences)?;
                    } else {
                        differences.push(SnapshotDifference {
                            path: field_path,
                            expected: Some(expected_value.clone()),
                            actual: None,
                            diff_type: SnapshotDifferenceType::Removed,
                        });
                    }
                }
                
                for (key, actual_value) in actual_obj {
                    if !expected_obj.contains_key(key) {
                        let field_path = if path.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        
                        differences.push(SnapshotDifference {
                            path: field_path,
                            expected: None,
                            actual: Some(actual_value.clone()),
                            diff_type: SnapshotDifferenceType::Added,
                        });
                    }
                }
            }
            (Value::Array(expected_arr), Value::Array(actual_arr)) => {
                let max_len = expected_arr.len().max(actual_arr.len());
                for i in 0..max_len {
                    let item_path = format!("{}[{}]", path, i);
                    
                    if i < expected_arr.len() && i < actual_arr.len() {
                        self.compare_values_recursive(&expected_arr[i], &actual_arr[i], &item_path, differences)?;
                    } else if i < expected_arr.len() {
                        differences.push(SnapshotDifference {
                            path: item_path,
                            expected: Some(expected_arr[i].clone()),
                            actual: None,
                            diff_type: SnapshotDifferenceType::Removed,
                        });
                    } else {
                        differences.push(SnapshotDifference {
                            path: item_path,
                            expected: None,
                            actual: Some(actual_arr[i].clone()),
                            diff_type: SnapshotDifferenceType::Added,
                        });
                    }
                }
            }
            (expected_val, actual_val) => {
                if expected_val != actual_val {
                    differences.push(SnapshotDifference {
                        path: path.to_string(),
                        expected: Some(expected_val.clone()),
                        actual: Some(actual_val.clone()),
                        diff_type: SnapshotDifferenceType::Changed,
                    });
                }
            }
        }
        Ok(())
    }

    /// Save snapshot
    fn save_snapshot(&self, path: &Path, data: &Value, metadata: &SnapshotMetadata) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| OpenApiToolError::ValidationError(
                    format!("Failed to create directory {}: {}", parent.display(), e)
                ))?;
        }
        
        let snapshot_data = SnapshotData {
            data: data.clone(),
            metadata: metadata.clone(),
        };
        
        let content = serde_json::to_string_pretty(&snapshot_data)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to serialize snapshot: {}", e)
            ))?;
        
        fs::write(path, content)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to write snapshot {}: {}", path.display(), e)
            ))
    }

    /// Get environment information
    fn get_environment_info(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("cargo_version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        env.insert("rust_version".to_string(), "unknown".to_string());
        env.insert("target".to_string(), "unknown".to_string());
        env.insert("host".to_string(), "unknown".to_string());
        env
    }
}

/// Snapshot data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotData {
    data: Value,
    metadata: SnapshotMetadata,
}

/// Snapshot comparison result
#[derive(Debug, Clone)]
pub struct SnapshotComparison {
    pub name: String,
    pub matches: bool,
    pub differences: Vec<SnapshotDifference>,
    pub snapshot_metadata: Option<SnapshotMetadata>,
}

/// Snapshot difference
#[derive(Debug, Clone)]
pub struct SnapshotDifference {
    pub path: String,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
    pub diff_type: SnapshotDifferenceType,
}

/// Snapshot difference type
#[derive(Debug, Clone)]
pub enum SnapshotDifferenceType {
    Added,
    Removed,
    Changed,
}

/// Snapshot information
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub name: String,
    pub path: PathBuf,
    pub metadata: SnapshotMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_snapshot_manager_creation() {
        let manager = SnapshotManager::with_defaults();
        assert_eq!(manager.config.format, SnapshotFormat::PrettyJson);
    }

    #[test]
    fn test_create_snapshot() {
        let manager = SnapshotManager::with_defaults();
        let data = json!({
            "message": "Hello, World!",
            "timestamp": "2023-01-01T00:00:00Z"
        });
        
        let metadata = manager.create_snapshot("test_snapshot", &data, Some("Test snapshot")).unwrap();
        assert_eq!(metadata.description, Some("Test snapshot".to_string()));
    }

    #[test]
    fn test_compare_with_snapshot() {
        let manager = SnapshotManager::with_defaults();
        let data = json!({
            "message": "Hello, World!",
            "timestamp": "2023-01-01T00:00:00Z"
        });
        
        // Create snapshot
        let _ = manager.create_snapshot("test_compare", &data, None).unwrap();
        
        // Compare with same data
        let comparison = manager.compare_with_snapshot("test_compare", &data).unwrap();
        assert!(comparison.matches);
        
        // Compare with different data
        let different_data = json!({
            "message": "Hello, Universe!",
            "timestamp": "2023-01-01T00:00:00Z"
        });
        
        let comparison = manager.compare_with_snapshot("test_compare", &different_data).unwrap();
        assert!(!comparison.matches);
        assert!(!comparison.differences.is_empty());
    }
}
