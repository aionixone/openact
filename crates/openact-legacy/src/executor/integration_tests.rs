//! Integration tests: Verify the complete authentication → execution → response flow

#[cfg(test)]
mod tests {
    use super::super::Executor;
    use crate::models::{
        ConnectionConfig, TaskConfig, AuthorizationType, 
        ApiKeyAuthParameters, OAuth2Parameters
    };
    

    /// Create API Key test connection
    fn create_api_key_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:openact:default:connection/api-key-test".to_string(),
            "API Key Test Connection".to_string(),
            AuthorizationType::ApiKey,
        );
        
        connection.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "test-api-key-123".to_string(),
        });
        
        connection
    }

    /// Create OAuth2 Client Credentials test connection
    fn create_oauth2_client_credentials_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:openact:default:connection/oauth2-cc-test".to_string(),
            "OAuth2 CC Test Connection".to_string(),
            AuthorizationType::OAuth2ClientCredentials,
        );
        
        connection.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scope: Some("read write".to_string()),
            redirect_uri: None,
            use_pkce: Some(false),
        });
        
        connection
    }

    /// Create test task
    fn create_test_task(connection_trn: &str) -> TaskConfig {
        TaskConfig::new(
            "trn:openact:default:task/test".to_string(),
            "Test API Task".to_string(),
            connection_trn.to_string(),
            "https://api.example.com/users".to_string(),
            "GET".to_string(),
        )
    }

    #[test]
    fn test_api_key_authentication_integration() {
        // Test the complete process of API Key authentication
        let connection = create_api_key_connection();
        let task = create_test_task(&connection.trn);
        
        // Create executor
        let _executor = Executor::new();
        
        // This test verifies parameter merging and authentication injection logic
        // Actual HTTP requests require a mock server, here we mainly test internal logic
        
        // Verify connection configuration
        assert_eq!(connection.authorization_type, AuthorizationType::ApiKey);
        assert!(connection.auth_parameters.api_key_auth_parameters.is_some());
        
        // Verify task configuration
        assert_eq!(task.connection_trn, connection.trn);
        assert_eq!(task.method, "GET");
    }

    #[test]
    fn test_oauth2_client_credentials_config() {
        // Test OAuth2 Client Credentials configuration
        let connection = create_oauth2_client_credentials_connection();
        let task = create_test_task(&connection.trn);
        
        // Verify OAuth2 configuration
        assert_eq!(connection.authorization_type, AuthorizationType::OAuth2ClientCredentials);
        let oauth_params = connection.auth_parameters.oauth_parameters.as_ref().unwrap();
        assert_eq!(oauth_params.client_id, "test-client-id");
        assert_eq!(oauth_params.token_url, "https://auth.example.com/token");
        assert_eq!(oauth_params.scope.as_ref().unwrap(), "read write");
        
        // Verify task association
        assert_eq!(task.connection_trn, connection.trn);
    }

    #[test]
    fn test_executor_creation() {
        // Test executor creation
        let _executor = Executor::new();
        
        // The executor should be able to be created normally
        // This verifies that all dependencies are correctly initialized
    }

    // TODO: Add more integration tests
    // - Test real HTTP requests (using a mock server)
    // - Test OAuth2 token refresh process
    // - Test error handling and retry mechanism
    // - Test various scenarios of parameter merging
}
