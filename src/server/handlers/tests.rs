#![cfg(test)]
#![cfg(feature = "server")]

use crate::utils::trn;
use crate::interface::dto::{ConnectionUpsertRequest, TaskUpsertRequest};
use crate::models::connection::{AuthorizationType, AuthParameters, ApiKeyAuthParameters};

#[test]
fn test_trn_validation() {
    // Valid TRNs
    assert!(trn::validate_trn("trn:openact:test-tenant:connection/mock").is_ok());
    assert!(trn::validate_trn("trn:openact:test-tenant:task/ping@v1").is_ok());
    
    // Invalid TRNs
    assert!(trn::validate_trn("invalid").is_err());
    assert!(trn::validate_trn("trn:wrong:tenant:resource/id").is_err());
    assert!(trn::validate_trn("trn:openact::resource/id").is_err()); // empty tenant
    assert!(trn::validate_trn("trn:openact:tenant:resource").is_err()); // no slash
    assert!(trn::validate_trn("trn:openact:tenant:/id").is_err()); // empty type
    assert!(trn::validate_trn("trn:openact:tenant:type/").is_err()); // empty id
}

#[test]
fn test_parse_connection_trn() {
    let (tenant, id) = trn::parse_connection_trn("trn:openact:test-tenant:connection/mock@v1").unwrap();
    assert_eq!(tenant, "test-tenant");
    assert_eq!(id, "mock@v1");
    
    assert!(trn::parse_connection_trn("trn:openact:tenant:task/id").is_err());
}

#[test]
fn test_parse_task_trn() {
    let (tenant, id) = trn::parse_task_trn("trn:openact:test-tenant:task/ping@v1").unwrap();
    assert_eq!(tenant, "test-tenant");
    assert_eq!(id, "ping@v1");
    
    assert!(trn::parse_task_trn("trn:openact:tenant:connection/id").is_err());
}

// Helper function to create test connection request
fn create_test_connection_request() -> ConnectionUpsertRequest {
    ConnectionUpsertRequest {
        trn: "trn:openact:test:connection/test@v1".to_string(),
        name: "Test Connection".to_string(),
        authorization_type: AuthorizationType::ApiKey,
        auth_parameters: AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "X-API-Key".to_string(),
                api_key_value: "test-key".to_string(),
            }),
            basic_auth_parameters: None,
            oauth_parameters: None,
        },
        invocation_http_parameters: None,
        network_config: None,
        timeout_config: None,
        http_policy: None,
        retry_policy: None,
        auth_ref: None,
    }
}

// Helper function to create test task request
fn create_test_task_request() -> TaskUpsertRequest {
    TaskUpsertRequest {
        trn: "trn:openact:test:task/test@v1".to_string(),
        name: "Test Task".to_string(),
        connection_trn: "trn:openact:test:connection/test@v1".to_string(),
        api_endpoint: "https://api.example.com/test".to_string(),
        method: "GET".to_string(),
        headers: None,
        query_params: None,
        request_body: None,
        timeout_config: None,
        network_config: None,
        http_policy: None,
        response_policy: None,
        retry_policy: None,
    }
}

// Test error response handling without full database integration
mod error_handling_tests {
    use crate::interface::error::helpers;

    #[test]
    fn test_validation_error_format() {
        let error = helpers::validation_error("invalid_trn", "TRN format is invalid");
        assert_eq!(error.code, "validation.invalid_trn");
        assert_eq!(error.message, "TRN format is invalid");
    }

    #[test]
    fn test_not_found_error_format() {
        let error = helpers::not_found_error("connection");
        assert_eq!(error.code, "not_found.connection");
        assert_eq!(error.message, "not found");
    }

    #[test]
    fn test_storage_error_format() {
        let error = helpers::storage_error("Database connection failed");
        assert_eq!(error.code, "internal.storage_error");
        assert_eq!(error.message, "Database connection failed");
    }

    #[test]
    fn test_execution_error_format() {
        let error = helpers::execution_error("HTTP request timeout");
        assert_eq!(error.code, "internal.execution_failed");
        assert_eq!(error.message, "HTTP request timeout");
    }
}

// Test DTO validation and conversion logic
mod dto_validation_tests {
    use super::*;

    #[test]
    fn test_connection_upsert_request_validation() {
        let req = create_test_connection_request();
        
        // Test basic structure
        assert_eq!(req.trn, "trn:openact:test:connection/test@v1");
        assert_eq!(req.name, "Test Connection");
        assert_eq!(req.authorization_type, AuthorizationType::ApiKey);
        
        // Test auth parameters
        assert!(req.auth_parameters.api_key_auth_parameters.is_some());
        let api_key = req.auth_parameters.api_key_auth_parameters.unwrap();
        assert_eq!(api_key.api_key_name, "X-API-Key");
        assert_eq!(api_key.api_key_value, "test-key");
    }

    #[test]
    fn test_task_upsert_request_validation() {
        let req = create_test_task_request();
        
        // Test basic structure
        assert_eq!(req.trn, "trn:openact:test:task/test@v1");
        assert_eq!(req.name, "Test Task");
        assert_eq!(req.connection_trn, "trn:openact:test:connection/test@v1");
        assert_eq!(req.api_endpoint, "https://api.example.com/test");
        assert_eq!(req.method, "GET");
    }

    #[test]
    fn test_connection_dto_to_config_conversion() {
        let req = create_test_connection_request();
        
        // Test creation (no existing data)
        let config = req.clone().to_config(None, None);
        assert_eq!(config.trn, req.trn);
        assert_eq!(config.name, req.name);
        assert_eq!(config.version, 1);
        assert_eq!(config.created_at, config.updated_at);
        
        // Test update (with existing data)
        use chrono::{DateTime, Utc};
        let existing_created_at = DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let config_update = req.to_config(Some(5), Some(existing_created_at));
        assert_eq!(config_update.version, 6);
        assert_eq!(config_update.created_at, existing_created_at);
        assert!(config_update.updated_at > existing_created_at);
    }

    #[test]
    fn test_task_dto_to_config_conversion() {
        let req = create_test_task_request();
        
        // Test creation
        let config = req.clone().to_config(None, None);
        assert_eq!(config.trn, req.trn);
        assert_eq!(config.connection_trn, req.connection_trn);
        assert_eq!(config.version, 1);
        
        // Test update with versioning
        use chrono::{DateTime, Utc};
        let existing_created_at = DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let config_update = req.to_config(Some(2), Some(existing_created_at));
        assert_eq!(config_update.version, 3);
        assert_eq!(config_update.created_at, existing_created_at);
    }
}

// Test TRN validation edge cases
mod trn_validation_tests {
    use crate::utils::trn;

    #[test]
    fn test_connection_trn_edge_cases() {
        // Valid edge cases
        assert!(trn::validate_trn("trn:openact:a:connection/b@v1").is_ok());
        assert!(trn::validate_trn("trn:openact:test-tenant-123:connection/test-conn-456@v99").is_ok());
        
        // Invalid edge cases - basic format validation only
        assert!(trn::validate_trn("trn:openact:test:connection/").is_err()); // empty id
        assert!(trn::validate_trn("trn:openact::connection/test@v1").is_err()); // empty tenant
        assert!(trn::validate_trn("trn:openact:test:/test@v1").is_err()); // empty resource type
        
        // Test specific connection parsing (stricter)
        assert!(trn::parse_connection_trn("trn:openact:test:task/test@v1").is_err()); // wrong type
        assert!(trn::parse_connection_trn("trn:openact:test:wrongtype/test@v1").is_err()); // wrong type
    }

    #[test]
    fn test_task_trn_edge_cases() {
        // Valid edge cases
        assert!(trn::validate_trn("trn:openact:a:task/b@v1").is_ok());
        assert!(trn::validate_trn("trn:openact:test-tenant:task/get-users@v2").is_ok());
        
        // Invalid edge cases - basic format validation
        assert!(trn::validate_trn("trn:openact:test:task/").is_err()); // empty name+version
        assert!(trn::validate_trn("trn:openact:test:").is_err()); // no slash at all
        
        // Test specific task parsing (stricter)
        assert!(trn::parse_task_trn("trn:openact:test:task/").is_err()); // empty id after parsing
        assert!(trn::parse_task_trn("trn:openact:test:connection/test@v1").is_err()); // wrong type
    }

    #[test]
    fn test_trn_parsing_consistency() {
        let connection_trn = "trn:openact:my-tenant:connection/my-conn@v1";
        let task_trn = "trn:openact:my-tenant:task/my-task@v1";
        
        assert!(trn::validate_trn(connection_trn).is_ok());
        assert!(trn::validate_trn(task_trn).is_ok());
        
        let (tenant, id) = trn::parse_connection_trn(connection_trn).unwrap();
        assert_eq!(tenant, "my-tenant");
        assert_eq!(id, "my-conn@v1");
        
        let (tenant, id) = trn::parse_task_trn(task_trn).unwrap();
        assert_eq!(tenant, "my-tenant");
        assert_eq!(id, "my-task@v1");
    }
}
