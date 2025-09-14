// Test runner for executing Action tests with Golden Playback

use super::golden_playback::*;
use crate::action::{ActionParser, ActionParsingOptions, ActionRunner, AuthAdapter};
use crate::spec::api_spec::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use crate::utils::error::Result;

/// Test runner for Action-related tests
pub struct ActionTestRunner {
    golden_playback: GoldenPlayback,
    auth_adapter: Option<Arc<AuthAdapter>>,
}

impl ActionTestRunner {
    /// Create a new test runner
    pub fn new(config: GoldenPlaybackConfig) -> Self {
        Self {
            golden_playback: GoldenPlayback::new(config),
            auth_adapter: None,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(GoldenPlaybackConfig::default())
    }

    /// Set authentication adapter
    pub fn set_auth_adapter(&mut self, auth_adapter: Arc<AuthAdapter>) {
        self.auth_adapter = Some(auth_adapter);
    }

    /// Test Action parsing
    pub async fn test_action_parsing(&self, test_name: &str, spec: &OpenApi30Spec) -> Result<GoldenPlaybackResult> {
        self.golden_playback.run_test(test_name, || async {
            let options = ActionParsingOptions {
                default_provider: "test".to_string(),
                default_tenant: "test_tenant".to_string(),
                include_deprecated: false,
                validate_schemas: true,
                extension_handlers: HashMap::new(),
                config_dir: Some("config".to_string()),
                provider_host: Some("api.github.com".to_string()),
            };
            
            let mut parser = ActionParser::new(options);
            let result = parser.parse_spec(spec)?;
            
            // Convert to JSON for comparison
            Ok(serde_json::to_value(result)?)
        }).await
    }

    /// Test Action execution
    pub async fn test_action_execution(
        &self,
        test_name: &str,
        action: &crate::action::Action,
        context: crate::action::ActionExecutionContext,
    ) -> Result<GoldenPlaybackResult> {
        self.golden_playback.run_test(test_name, || async {
            let mut runner = ActionRunner::with_tenant("test_tenant".to_string());
            
            if let Some(auth_adapter) = &self.auth_adapter {
                runner.set_auth_adapter(auth_adapter.clone());
            }
            
            let result = runner.execute_action(action, context).await?;
            
            // Convert to JSON for comparison
            Ok(serde_json::to_value(result)?)
        }).await
    }

    /// Test authentication flow
    pub async fn test_auth_flow(
        &self,
        test_name: &str,
        auth_config: &crate::action::AuthConfig,
    ) -> Result<GoldenPlaybackResult> {
        self.golden_playback.run_test(test_name, || async {
            let adapter = AuthAdapter::new("test_tenant".to_string());
            let auth_context = adapter.get_auth_for_action(auth_config).await?;
            
            // Convert to JSON for comparison
            Ok(serde_json::to_value(auth_context)?)
        }).await
    }

    /// Test extension field processing
    pub async fn test_extension_processing(
        &self,
        test_name: &str,
        extensions: &HashMap<String, Value>,
    ) -> Result<GoldenPlaybackResult> {
        self.golden_playback.run_test(test_name, || async {
            use crate::action::extensions::ExtensionRegistry;
            
            let processor = ExtensionRegistry::create_default_processor();
            let processed = processor.process_extensions(extensions)?;
            
            // Convert to JSON for comparison
            Ok(serde_json::to_value(processed)?)
        }).await
    }

    /// Run all tests in a test suite
    pub async fn run_test_suite(&self, test_suite: &TestSuite) -> Result<TestSuiteResult> {
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let mut updated = 0;
        let mut new = 0;

        for test_case in &test_suite.tests {
            let result = match &test_case.test_type {
                TestType::ActionParsing { spec } => {
                    self.test_action_parsing(&test_case.name, spec).await?
                }
                TestType::ActionExecution { action, context } => {
                    self.test_action_execution(&test_case.name, action, context.clone()).await?
                }
                TestType::AuthFlow { auth_config } => {
                    self.test_auth_flow(&test_case.name, auth_config).await?
                }
                TestType::ExtensionProcessing { extensions } => {
                    self.test_extension_processing(&test_case.name, extensions).await?
                }
            };

            match result.status {
                TestStatus::Passed => passed += 1,
                TestStatus::Failed => failed += 1,
                TestStatus::Updated => updated += 1,
                TestStatus::New => new += 1,
            }

            results.push(result);
        }

        Ok(TestSuiteResult {
            suite_name: test_suite.name.clone(),
            results,
            summary: TestSummary {
                total: test_suite.tests.len(),
                passed,
                failed,
                updated,
                new,
            },
        })
    }
}

/// Test suite definition
#[derive(Debug, Clone)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
}

/// Individual test case
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub description: Option<String>,
    pub test_type: TestType,
}

/// Type of test
#[derive(Debug, Clone)]
pub enum TestType {
    ActionParsing { spec: OpenApi30Spec },
    ActionExecution { 
        action: crate::action::Action,
        context: crate::action::ActionExecutionContext,
    },
    AuthFlow { auth_config: crate::action::AuthConfig },
    ExtensionProcessing { extensions: HashMap<String, Value> },
}

/// Test suite result
#[derive(Debug, Clone)]
pub struct TestSuiteResult {
    pub suite_name: String,
    pub results: Vec<GoldenPlaybackResult>,
    pub summary: TestSummary,
}

/// Test summary
#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub updated: usize,
    pub new: usize,
}

impl TestSummary {
    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64) / (self.total as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_action_test_runner() {
        // Use temp golden dir to ensure New on first run
        let mut cfg = GoldenPlaybackConfig::default();
        let tmp = tempfile::tempdir().unwrap();
        cfg.golden_dir = tmp.keep();
        let runner = ActionTestRunner::new(cfg);
        
        // Create a simple test spec
        let spec = create_test_spec();
        
        let result = runner.test_action_parsing("test_simple_parsing", &spec).await.unwrap();
        assert_eq!(result.status, TestStatus::New);
    }

    fn create_test_spec() -> OpenApi30Spec {
        OpenApi30Spec {
            openapi: "3.0.0".to_string(),
            info: Info {
                title: "Test API".to_string(),
                version: "1.0.0".to_string(),
                description: Some("Test API for Golden Playback".to_string()),
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: HashMap::new(),
            },
            external_docs: None,
            servers: vec![],
            security: vec![],
            tags: vec![],
            paths: Paths {
                paths: {
                    let mut paths = HashMap::new();
                    paths.insert("/test".to_string(), PathItem {
                        reference: None,
                        summary: None,
                        description: None,
                        get: Some(Operation {
                            tags: vec![],
                            summary: None,
                            description: None,
                            external_docs: None,
                            operation_id: Some("test".to_string()),
                            parameters: vec![],
                            request_body: None,
                            responses: Responses::new(),
                            callbacks: HashMap::new(),
                            deprecated: false,
                            security: vec![],
                            servers: vec![],
                            extensions: HashMap::new(),
                        }),
                        put: None,
                        post: None,
                        delete: None,
                        options: None,
                        head: None,
                        patch: None,
                        trace: None,
                        servers: vec![],
                        parameters: vec![],
                        extensions: HashMap::new(),
                    });
                    paths
                },
                extensions: HashMap::new(),
            },
            components: None,
            extensions: HashMap::new(),
        }
    }
}
