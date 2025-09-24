#![cfg(test)]
#![cfg(feature = "server")]

use crate::interface::dto::{ConnectionUpsertRequest, TaskUpsertRequest};
use crate::models::connection::{ApiKeyAuthParameters, AuthParameters, AuthorizationType};
use crate::utils::trn;

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
    let (tenant, id) =
        trn::parse_connection_trn("trn:openact:test-tenant:connection/mock@v1").unwrap();
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
// HTTP Handler Integration Tests
#[cfg(feature = "server")]
mod http_handler_tests {
    use super::*;
    use crate::app::service::OpenActService;
    use crate::server::handlers;
    use crate::store::{DatabaseManager, StorageService};
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    async fn setup_test_service() -> OpenActService {
        // 设置环境变量一次
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            unsafe {
                std::env::set_var(
                    "OPENACT_MASTER_KEY",
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                );
                std::env::set_var("OPENACT_DB_URL", "sqlite::memory:");
            }
        });
        
        // 每个测试都创建新的内存数据库实例
        let db = DatabaseManager::new("sqlite::memory:").await.unwrap();
        let storage = std::sync::Arc::new(StorageService::new(db));
        OpenActService::from_storage(storage)
    }

    fn create_test_router() -> Router {
        Router::new()
            .route(
                "/api/v1/connections",
                axum::routing::post(handlers::connections::create),
            )
            .route(
                "/api/v1/connections",
                axum::routing::get(handlers::connections::list),
            )
            .route(
                "/api/v1/connections/{trn}",
                axum::routing::get(handlers::connections::get),
            )
            .route(
                "/api/v1/connections/{trn}",
                axum::routing::put(handlers::connections::update),
            )
            .route(
                "/api/v1/connections/{trn}/status",
                axum::routing::get(handlers::connections::status),
            )
            .route(
                "/api/v1/tasks",
                axum::routing::post(handlers::tasks::create),
            )
            .route("/api/v1/tasks", axum::routing::get(handlers::tasks::list))
            .route(
                "/api/v1/tasks/{trn}",
                axum::routing::get(handlers::tasks::get),
            )
            .route(
                "/api/v1/tasks/{trn}",
                axum::routing::put(handlers::tasks::update),
            )
            .route(
                "/api/v1/tasks/{trn}/execute",
                axum::routing::post(handlers::execute::execute),
            )
            .route(
                "/api/v1/execute",
                axum::routing::post(handlers::execute::execute_adhoc),
            )
    }

    #[tokio::test]
    async fn test_connection_create_success() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let req_body = json!(create_test_connection_request());
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/connections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&req_body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["trn"], "trn:openact:test:connection/test@v1");
        assert_eq!(result["version"], 1);
        assert!(result["created_at"].is_string());
        assert!(result["updated_at"].is_string());
    }

    #[tokio::test]
    async fn test_connection_create_invalid_trn() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let mut req = create_test_connection_request();
        req.trn = "invalid-trn".to_string();

        let req_body = json!(req);
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/connections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&req_body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["error_code"], "validation.invalid_input");
        assert!(result["hints"].is_array());
    }

    #[tokio::test]
    async fn test_connection_get_not_found() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/connections/trn:openact:test:connection/nonexistent")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let response_status = response.status();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body);
        println!("Response status: {}, body: '{}'", response_status, body_str);

        assert_eq!(response_status, StatusCode::NOT_FOUND);

        if !body.is_empty() {
            let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(result["error_code"], "not_found.connection");
            assert!(result["hints"].is_array());
        }
    }

    #[tokio::test]
    async fn test_connection_get_invalid_trn() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/connections/invalid-trn")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["error_code"], "validation.invalid_trn");
    }

    #[tokio::test]
    async fn test_task_create_success() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        // First create the required connection
        let conn_req = json!(create_test_connection_request());
        let conn_request = Request::builder()
            .method("POST")
            .uri("/api/v1/connections")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&conn_req).unwrap()))
            .unwrap();
        
        let conn_response = app.clone().oneshot(conn_request).await.unwrap();
        assert_eq!(conn_response.status(), StatusCode::CREATED);

        // Now create the task
        let task_req = json!(create_test_task_request());
        let task_request = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&task_req).unwrap()))
            .unwrap();

        let response = app.oneshot(task_request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        
        if status != StatusCode::CREATED {
            let error_msg = String::from_utf8_lossy(&body);
            panic!("Expected CREATED but got {:?}. Body: {}", status, error_msg);
        }
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["trn"], "trn:openact:test:task/test@v1");
        assert_eq!(result["version"], 1);
    }

    #[tokio::test]
    async fn test_task_create_invalid_connection_trn() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let mut req = create_test_task_request();
        req.connection_trn = "invalid-conn-trn".to_string();

        let req_body = json!(req);
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&req_body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(result["error_code"], "validation.invalid_input");
    }

    #[tokio::test]
    async fn test_connections_list_empty() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/connections")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        if status != StatusCode::OK {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let error_msg = String::from_utf8_lossy(&body);
            panic!("Expected OK but got {:?}. Body: {}", status, error_msg);
        }

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_tasks_list_empty() {
        let _service = setup_test_service().await;
        let app = create_test_router();

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/tasks")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        if status != StatusCode::OK {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let error_msg = String::from_utf8_lossy(&body);
            panic!("Expected OK but got {:?}. Body: {}", status, error_msg);
        }

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(result.is_array());
        if result.as_array().unwrap().len() != 0 {
            eprintln!("Expected empty tasks list but found: {}", serde_json::to_string_pretty(&result).unwrap());
        }
        assert_eq!(result.as_array().unwrap().len(), 0);
    }
}

mod trn_validation_tests {
    use crate::utils::trn;

    #[test]
    fn test_connection_trn_edge_cases() {
        // Valid edge cases
        assert!(trn::validate_trn("trn:openact:a:connection/b@v1").is_ok());
        assert!(
            trn::validate_trn("trn:openact:test-tenant-123:connection/test-conn-456@v99").is_ok()
        );

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
