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
// Shared test helper functions
#[cfg(feature = "server")]
async fn create_shared_test_router() -> axum::Router {
    use crate::app::service::OpenActService;
    use crate::server::handlers;
    use crate::store::{DatabaseManager, StorageService};
    use axum::Router;

    // Setup test service
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let test_id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let test_db_file = format!("/tmp/openact_test_handler_{}.db", test_id);

    let _ = std::fs::remove_file(&test_db_file);
    let test_db_url = format!("sqlite://{}", test_db_file);

    unsafe {
        std::env::set_var(
            "OPENACT_MASTER_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        std::env::set_var("OPENACT_DB_URL", &test_db_url);
    }

    let db = DatabaseManager::new(&test_db_url).await.unwrap();
    let storage = std::sync::Arc::new(StorageService::new(db));
    let service = OpenActService::from_storage(storage);

    Router::new()
        .route(
            "/api/v1/connections",
            axum::routing::post(handlers::connections::create).get(handlers::connections::list),
        )
        .route(
            "/api/v1/connections/{trn}",
            axum::routing::get(handlers::connections::get).put(handlers::connections::update),
        )
        .route(
            "/api/v1/connections/{trn}/status",
            axum::routing::get(handlers::connections::status),
        )
        .route(
            "/api/v1/tasks",
            axum::routing::post(handlers::tasks::create).get(handlers::tasks::list),
        )
        .route(
            "/api/v1/tasks/{trn}",
            axum::routing::get(handlers::tasks::get).put(handlers::tasks::update),
        )
        // System endpoints
        .route(
            "/api/v1/system/stats",
            axum::routing::get(handlers::system::stats),
        )
        .route(
            "/api/v1/system/health",
            axum::routing::get(handlers::system::health),
        )
        .route(
            "/api/v1/system/cleanup",
            axum::routing::post(handlers::system::cleanup),
        )
        // Execute endpoints
        .route(
            "/api/v1/tasks/{trn}/execute",
            axum::routing::post(handlers::execute::execute),
        )
        .route(
            "/api/v1/execute/adhoc",
            axum::routing::post(handlers::execute::execute_adhoc),
        )
        .with_state(service)
}

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

    #[allow(dead_code)]
    pub async fn setup_test_service() -> OpenActService {
        // 为每个测试创建唯一的数据库文件，避免内存数据库的连接问题
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let test_id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let test_db_file = format!("/tmp/openact_test_handler_{}.db", test_id);

        // 清理之前的测试数据
        let _ = std::fs::remove_file(&test_db_file);

        let test_db_url = format!("sqlite://{}", test_db_file);

        // 设置环境变量（每个测试使用自己的数据库）
        unsafe {
            std::env::set_var(
                "OPENACT_MASTER_KEY",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            );
            std::env::set_var("OPENACT_DB_URL", &test_db_url);
        }

        // 创建服务实例（会自动运行迁移）
        let db = DatabaseManager::new(&test_db_url).await.unwrap();
        let storage = std::sync::Arc::new(StorageService::new(db));
        OpenActService::from_storage(storage)
    }

    /// 创建注入了服务状态的测试路由器（新版本）
    #[allow(dead_code)]
    pub async fn create_test_router_with_service() -> Router {
        let service = setup_test_service().await;

        Router::new()
            .route(
                "/api/v1/connections",
                axum::routing::post(handlers::connections::create).get(handlers::connections::list),
            )
            .route(
                "/api/v1/connections/{trn}",
                axum::routing::get(handlers::connections::get).put(handlers::connections::update),
            )
            .route(
                "/api/v1/connections/{trn}/status",
                axum::routing::get(handlers::connections::status),
            )
            .route(
                "/api/v1/tasks",
                axum::routing::post(handlers::tasks::create).get(handlers::tasks::list),
            )
            .route(
                "/api/v1/tasks/{trn}",
                axum::routing::get(handlers::tasks::get).put(handlers::tasks::update),
            )
            // System endpoints
            .route(
                "/api/v1/system/stats",
                axum::routing::get(handlers::system::stats),
            )
            .route(
                "/api/v1/system/health",
                axum::routing::get(handlers::system::health),
            )
            .route(
                "/api/v1/system/cleanup",
                axum::routing::post(handlers::system::cleanup),
            )
            .with_state(service)
    }

    #[tokio::test]
    async fn test_connection_create_success() {
        let app = super::create_shared_test_router().await;

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
        let app = super::create_shared_test_router().await;

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
        let app = super::create_shared_test_router().await;

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
        let app = super::create_shared_test_router().await;

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
        let app = super::create_shared_test_router().await;

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
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

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
        let app = super::create_shared_test_router().await;

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
        let app = super::create_shared_test_router().await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/connections")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        if status != StatusCode::OK {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
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
        let app = super::create_shared_test_router().await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/tasks")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        if status != StatusCode::OK {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let error_msg = String::from_utf8_lossy(&body);
            panic!("Expected OK but got {:?}. Body: {}", status, error_msg);
        }

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(result.is_array());
        if result.as_array().unwrap().len() != 0 {
            eprintln!(
                "Expected empty tasks list but found: {}",
                serde_json::to_string_pretty(&result).unwrap()
            );
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

    // System HTTP handler tests
    #[tokio::test]
    async fn test_system_stats_success() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/api/v1/system/stats")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Basic structure verification
        assert!(json.get("storage").is_some());
        assert!(json.get("caches").is_some());
        assert!(json.get("client_pool").is_some());
        assert!(json.get("timestamp").is_some());
    }

    #[tokio::test]
    async fn test_system_health_success() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/api/v1/system/health")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Health response structure
        assert!(json.get("status").is_some());
        assert!(json.get("timestamp").is_some());
        assert!(json.get("components").is_some());

        let status = json["status"].as_str().unwrap();
        assert!(status == "healthy" || status == "unhealthy");
    }

    #[tokio::test]
    async fn test_system_cleanup_success() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/system/cleanup")
            .header("content-type", "application/json")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Cleanup response structure
        assert!(json.get("message").is_some());
        assert!(json.get("cleaned_count").is_some());
        assert!(json.get("timestamp").is_some());

        let message = json["message"].as_str().unwrap();
        assert!(message.contains("cleanup"));
    }

    // Execute HTTP handler tests
    #[tokio::test]
    async fn test_execute_invalid_trn() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/invalid-trn/execute")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"overrides": {}}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Invalid TRN format validation returns 400 Bad Request
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "validation.invalid_trn");
    }

    #[tokio::test]
    async fn test_execute_adhoc_missing_connection_trn() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/execute/adhoc")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"method": "GET", "endpoint": "https://api.example.com"}"#,
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Missing required field results in 422 Unprocessable Entity
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn test_execute_adhoc_invalid_connection_trn() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/execute/adhoc")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"connection_trn": "invalid-trn", "method": "GET", "endpoint": "https://api.example.com"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Invalid TRN format validation returns 400 Bad Request
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "validation.invalid_connection_trn");
    }

    #[tokio::test]
    async fn test_execute_adhoc_empty_method() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/execute/adhoc")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"connection_trn": "trn:openact:test:connection/test@v1", "method": "", "endpoint": "https://api.example.com"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Empty method validation returns 400 Bad Request
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "validation.missing_method");
    }

    #[tokio::test]
    async fn test_execute_adhoc_empty_endpoint() {
        use tower::ServiceExt;
        let app = super::create_shared_test_router().await;

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/execute/adhoc")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"connection_trn": "trn:openact:test:connection/test@v1", "method": "GET", "endpoint": ""}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Empty endpoint validation returns 400 Bad Request
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "validation.missing_endpoint");
    }
}
