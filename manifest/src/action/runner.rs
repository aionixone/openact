// Action runner implementation
// Handles execution of actions with TRN integration

use super::models::*;
use super::auth::AuthAdapter;
use crate::utils::error::{OpenApiToolError, Result};
use serde_json::Value;
use std::sync::Arc;

/// Action runner for executing actions
pub struct ActionRunner {
    /// Execution timeout in milliseconds
    timeout_ms: u64,
    /// Maximum retry attempts
    max_retries: u32,
    /// Authentication adapter for handling auth
    auth_adapter: Option<Arc<AuthAdapter>>,
    /// Tenant identifier
    #[allow(dead_code)]
    tenant: String,
}

impl ActionRunner {
    /// Create a new action runner
    pub fn new() -> Self {
        Self {
            timeout_ms: 30000, // 30 seconds default
            max_retries: 3,
            auth_adapter: None,
            tenant: "default".to_string(),
        }
    }

    /// Create a new action runner with tenant
    pub fn with_tenant(tenant: String) -> Self {
        Self {
            timeout_ms: 30000,
            max_retries: 3,
            auth_adapter: None,
            tenant,
        }
    }

    /// Set the authentication adapter
    pub fn set_auth_adapter(&mut self, auth_adapter: Arc<AuthAdapter>) {
        self.auth_adapter = Some(auth_adapter);
    }

    /// Create a new action runner with custom timeout
    pub fn with_timeout(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            max_retries: 3,
            auth_adapter: None,
            tenant: "default".to_string(),
        }
    }

    /// Execute an action
    pub async fn execute_action(
        &self,
        action: &Action,
        context: ActionExecutionContext,
    ) -> Result<ActionExecutionResult> {
        let start_time = std::time::Instant::now();
        
        // Create execution result
        let result = ActionExecutionResult::new(
            context.execution_trn.clone(),
            ExecutionStatus::Running,
        );

        // Validate action
        if let Err(e) = action.validate() {
            return Ok(result
                .set_error_message(format!("Action validation failed: {}", e))
                .set_duration(start_time.elapsed().as_millis() as u64));
        }

        // Validate context
        if let Err(e) = self.validate_context(&context) {
            return Ok(result
                .set_error_message(format!("Context validation failed: {}", e))
                .set_duration(start_time.elapsed().as_millis() as u64));
        }

        // Execute the action (placeholder implementation)
        match self.execute_action_impl(action, context).await {
            Ok(response_data) => {
                Ok(result
                    .set_response_data(response_data)
                    .set_status_code(200)
                    .set_duration(start_time.elapsed().as_millis() as u64))
            }
            Err(e) => {
                Ok(result
                    .set_error_message(e.to_string())
                    .set_duration(start_time.elapsed().as_millis() as u64))
            }
        }
    }

    /// Validate execution context
    fn validate_context(&self, context: &ActionExecutionContext) -> Result<()> {
        if context.action_trn.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Action TRN cannot be empty".to_string()
            ));
        }

        if context.execution_trn.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Execution TRN cannot be empty".to_string()
            ));
        }

        if context.tenant.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Tenant cannot be empty".to_string()
            ));
        }

        if context.provider.trim().is_empty() {
            return Err(OpenApiToolError::ValidationError(
                "Provider cannot be empty".to_string()
            ));
        }

        Ok(())
    }

    /// Execute action implementation with authentication
    async fn execute_action_impl(
        &self,
        action: &Action,
        context: ActionExecutionContext,
    ) -> Result<Value> {
        // 1. Get authentication context if needed
        let auth_context = if let Some(auth_config) = &action.auth_config {
            if let Some(adapter) = &self.auth_adapter {
                Some(adapter.get_auth_for_action(auth_config).await?)
            } else {
                return Err(OpenApiToolError::ValidationError(
                    "Authentication required but no auth adapter configured".to_string()
                ));
            }
        } else {
            None
        };

        // 2. Build HTTP request headers
        let mut headers = context.headers.clone();
        if let Some(auth) = &auth_context {
            // Add authentication header
            headers.insert("Authorization".to_string(), auth.get_auth_header());
            
            // Add any additional headers from auth context
            for (key, value) in &auth.headers {
                headers.insert(key.clone(), value.clone());
            }
        }

        // 3. Build the HTTP request (placeholder implementation)
        // In a real implementation, this would use an HTTP client like reqwest
        let request_info = serde_json::json!({
            "method": action.method,
            "path": action.path,
            "headers": headers,
            "auth_provider": auth_context.as_ref().map(|a| &a.provider),
            "auth_type": action.auth_config.as_ref().map(|a| &a.auth_type),
            "parameters": context.parameters,
            "timestamp": context.timestamp,
            "status": "executed"
        });

        Ok(request_info)
    }

    /// Set execution timeout
    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Set maximum retry attempts
    pub fn set_max_retries(&mut self, max_retries: u32) {
        self.max_retries = max_retries;
    }

    /// Get execution timeout
    pub fn get_timeout(&self) -> u64 {
        self.timeout_ms
    }

    /// Get maximum retry attempts
    pub fn get_max_retries(&self) -> u32 {
        self.max_retries
    }
}

impl Default for ActionRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_action() -> Action {
        Action::new(
            "get_user".to_string(),
            "GET".to_string(),
            "/users/{id}".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
        )
    }

    fn create_test_context() -> ActionExecutionContext {
        ActionExecutionContext::new(
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
            "trn:stepflow:tenant123:execution:action-execution:exec-123".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        )
    }

    #[tokio::test]
    async fn test_action_runner_creation() {
        let runner = ActionRunner::new();
        assert_eq!(runner.get_timeout(), 30000);
        assert_eq!(runner.get_max_retries(), 3);
    }

    #[tokio::test]
    async fn test_action_runner_with_timeout() {
        let runner = ActionRunner::with_timeout(60000);
        assert_eq!(runner.get_timeout(), 60000);
        assert_eq!(runner.get_max_retries(), 3);
    }

    #[tokio::test]
    async fn test_execute_action() {
        let runner = ActionRunner::new();
        let action = create_test_action();
        let context = create_test_context();

        let result = runner.execute_action(&action, context).await.unwrap();

        assert_eq!(result.execution_trn, "trn:stepflow:tenant123:execution:action-execution:exec-123");
        assert!(matches!(result.status, ExecutionStatus::Success));
        assert!(result.response_data.is_some());
        assert!(result.duration_ms.is_some());
    }

    #[tokio::test]
    async fn test_context_validation() {
        let runner = ActionRunner::new();
        let action = create_test_action();
        
        // Test empty action TRN
        let mut context = create_test_context();
        context.action_trn = "".to_string();
        
        let result = runner.execute_action(&action, context).await.unwrap();
        assert!(matches!(result.status, ExecutionStatus::Failed));
        assert!(result.error_message.is_some());
    }

    #[tokio::test]
    async fn test_runner_configuration() {
        let mut runner = ActionRunner::new();
        
        runner.set_timeout(45000);
        runner.set_max_retries(5);
        
        assert_eq!(runner.get_timeout(), 45000);
        assert_eq!(runner.get_max_retries(), 5);
    }
}
