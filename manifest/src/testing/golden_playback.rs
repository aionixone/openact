// Golden Playback testing implementation
// Records test results and compares them for regression testing

use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::utils::error::{OpenApiToolError, Result};
use chrono::{DateTime, Utc};

/// Golden Playback test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenPlaybackConfig {
    /// Directory to store golden files
    pub golden_dir: PathBuf,
    /// Whether to update golden files on mismatch
    pub update_on_mismatch: bool,
    /// Whether to ignore timestamp differences
    pub ignore_timestamps: bool,
    /// Whether to ignore dynamic fields (like IDs, tokens)
    pub ignore_dynamic_fields: bool,
    /// Fields to ignore during comparison
    pub ignored_fields: Vec<String>,
}

impl Default for GoldenPlaybackConfig {
    fn default() -> Self {
        Self {
            golden_dir: PathBuf::from("testdata/golden"),
            update_on_mismatch: false,
            ignore_timestamps: true,
            ignore_dynamic_fields: true,
            ignored_fields: vec![
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

/// Golden Playback test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenPlaybackResult {
    /// Test name
    pub test_name: String,
    /// Test status
    pub status: TestStatus,
    /// Expected result (golden)
    pub expected: Option<Value>,
    /// Actual result
    pub actual: Option<Value>,
    /// Differences found
    pub differences: Vec<Difference>,
    /// Test metadata
    pub metadata: TestMetadata,
}

/// Test status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
    Updated,
    New,
}

/// Difference between expected and actual results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    /// Path to the different field
    pub path: String,
    /// Expected value
    pub expected: Value,
    /// Actual value
    pub actual: Value,
    /// Difference type
    pub diff_type: DifferenceType,
}

/// Type of difference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DifferenceType {
    Added,
    Removed,
    Changed,
    TypeMismatch,
}

/// Test metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetadata {
    /// Test execution time
    pub executed_at: DateTime<Utc>,
    /// Test duration in milliseconds
    pub duration_ms: u64,
    /// Test environment info
    pub environment: HashMap<String, String>,
    /// Test version
    pub version: String,
}

/// Golden Playback test manager
pub struct GoldenPlayback {
    config: GoldenPlaybackConfig,
}

impl GoldenPlayback {
    /// Create a new Golden Playback instance
    pub fn new(config: GoldenPlaybackConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(GoldenPlaybackConfig::default())
    }

    /// Run a test with Golden Playback
    pub async fn run_test<F, Fut>(&self, test_name: &str, test_fn: F) -> Result<GoldenPlaybackResult>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value>>,
    {
        let start_time = std::time::Instant::now();
        
        // Execute the test
        let actual_result = test_fn().await?;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        // Load or create golden file
        let golden_path = self.get_golden_path(test_name);
        let expected_result = self.load_golden_file(&golden_path).ok();
        
        // Compare results
        let (status, differences) = if let Some(expected) = &expected_result {
            let differences = self.compare_values(expected, &actual_result)?;
            if differences.is_empty() {
                (TestStatus::Passed, differences)
            } else {
                (TestStatus::Failed, differences)
            }
        } else {
            // New test - no golden file exists
            (TestStatus::New, vec![])
        };
        
        // Update golden file if needed
        if matches!(status, TestStatus::New) || 
           (matches!(status, TestStatus::Failed) && self.config.update_on_mismatch) {
            self.save_golden_file(&golden_path, &actual_result)?;
        }
        
        // Create test metadata
        let metadata = TestMetadata {
            executed_at: Utc::now(),
            duration_ms,
            environment: self.get_environment_info(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        
        Ok(GoldenPlaybackResult {
            test_name: test_name.to_string(),
            status,
            expected: expected_result,
            actual: Some(actual_result),
            differences,
            metadata,
        })
    }

    /// Compare two JSON values
    fn compare_values(&self, expected: &Value, actual: &Value) -> Result<Vec<Difference>> {
        let mut differences = Vec::new();
        self.compare_values_recursive(expected, actual, "", &mut differences)?;
        Ok(differences)
    }

    /// Recursively compare JSON values
    fn compare_values_recursive(
        &self,
        expected: &Value,
        actual: &Value,
        path: &str,
        differences: &mut Vec<Difference>,
    ) -> Result<()> {
        // Skip ignored fields
        if self.config.ignored_fields.iter().any(|field| path.ends_with(field)) {
            return Ok(());
        }

        match (expected, actual) {
            (Value::Object(expected_obj), Value::Object(actual_obj)) => {
                // Compare object fields
                for (key, expected_value) in expected_obj {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    
                    if let Some(actual_value) = actual_obj.get(key) {
                        self.compare_values_recursive(expected_value, actual_value, &field_path, differences)?;
                    } else {
                        differences.push(Difference {
                            path: field_path,
                            expected: expected_value.clone(),
                            actual: Value::Null,
                            diff_type: DifferenceType::Removed,
                        });
                    }
                }
                
                // Check for new fields in actual
                for (key, actual_value) in actual_obj {
                    if !expected_obj.contains_key(key) {
                        let field_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        
                        differences.push(Difference {
                            path: field_path,
                            expected: Value::Null,
                            actual: actual_value.clone(),
                            diff_type: DifferenceType::Added,
                        });
                    }
                }
            }
            (Value::Array(expected_arr), Value::Array(actual_arr)) => {
                // Compare arrays
                let max_len = expected_arr.len().max(actual_arr.len());
                for i in 0..max_len {
                    let item_path = format!("{}[{}]", path, i);
                    
                    if i < expected_arr.len() && i < actual_arr.len() {
                        self.compare_values_recursive(&expected_arr[i], &actual_arr[i], &item_path, differences)?;
                    } else if i < expected_arr.len() {
                        differences.push(Difference {
                            path: item_path,
                            expected: expected_arr[i].clone(),
                            actual: Value::Null,
                            diff_type: DifferenceType::Removed,
                        });
                    } else {
                        differences.push(Difference {
                            path: item_path,
                            expected: Value::Null,
                            actual: actual_arr[i].clone(),
                            diff_type: DifferenceType::Added,
                        });
                    }
                }
            }
            (expected_val, actual_val) => {
                // Compare primitive values
                if expected_val != actual_val {
                    differences.push(Difference {
                        path: path.to_string(),
                        expected: expected_val.clone(),
                        actual: actual_val.clone(),
                        diff_type: DifferenceType::Changed,
                    });
                }
            }
        }
        
        Ok(())
    }

    /// Get the path for a golden file
    fn get_golden_path(&self, test_name: &str) -> PathBuf {
        self.config.golden_dir.join(format!("{}.json", test_name))
    }

    /// Load a golden file
    fn load_golden_file(&self, path: &Path) -> Result<Value> {
        let content = fs::read_to_string(path)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to read golden file {}: {}", path.display(), e)
            ))?;
        
        serde_json::from_str(&content)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to parse golden file {}: {}", path.display(), e)
            ))
    }

    /// Save a golden file
    fn save_golden_file(&self, path: &Path, value: &Value) -> Result<()> {
        // Create directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| OpenApiToolError::ValidationError(
                    format!("Failed to create directory {}: {}", parent.display(), e)
                ))?;
        }
        
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to serialize golden file: {}", e)
            ))?;
        
        fs::write(path, content)
            .map_err(|e| OpenApiToolError::ValidationError(
                format!("Failed to write golden file {}: {}", path.display(), e)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_golden_playback_new_test() {
        let golden = GoldenPlayback::with_defaults();
        
        let result = golden.run_test("test_new", || async {
            Ok(json!({
                "message": "Hello, World!",
                "timestamp": "2023-01-01T00:00:00Z"
            }))
        }).await.unwrap();
        
        assert_eq!(result.status, TestStatus::New);
        assert_eq!(result.test_name, "test_new");
    }

    #[tokio::test]
    async fn test_golden_playback_passing_test() {
        let golden = GoldenPlayback::with_defaults();
        
        // First run - creates golden file
        let _ = golden.run_test("test_passing", || async {
            Ok(json!({
                "message": "Hello, World!",
                "timestamp": "2023-01-01T00:00:00Z"
            }))
        }).await.unwrap();
        
        // Second run - should pass
        let result = golden.run_test("test_passing", || async {
            Ok(json!({
                "message": "Hello, World!",
                "timestamp": "2023-01-02T00:00:00Z" // Different timestamp, but ignored
            }))
        }).await.unwrap();
        
        assert_eq!(result.status, TestStatus::Passed);
    }

    #[tokio::test]
    async fn test_golden_playback_failing_test() {
        let golden = GoldenPlayback::with_defaults();
        
        // First run - creates golden file
        let _ = golden.run_test("test_failing", || async {
            Ok(json!({
                "message": "Hello, World!",
                "count": 42
            }))
        }).await.unwrap();
        
        // Second run - should fail
        let result = golden.run_test("test_failing", || async {
            Ok(json!({
                "message": "Hello, Universe!", // Different message
                "count": 42
            }))
        }).await.unwrap();
        
        assert_eq!(result.status, TestStatus::Failed);
        assert!(!result.differences.is_empty());
    }
}
