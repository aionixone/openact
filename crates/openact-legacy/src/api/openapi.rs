//! OpenAPI documentation configuration and utilities
//!
//! This module provides the core OpenAPI configuration, including:
//! - API metadata and server information
//! - Schema definitions and components
//! - Tag organization and documentation
//! - Authentication scheme definitions

use utoipa::{OpenApi, Modify};

/// Main OpenAPI specification for OpenAct
#[derive(OpenApi)]
#[openapi(
    paths(
        // Connection endpoints
        crate::server::handlers::connections::list,
        crate::server::handlers::connections::create,
        crate::server::handlers::connections::get,
        crate::server::handlers::connections::update,
        crate::server::handlers::connections::del,
        crate::server::handlers::connections::status,
        crate::server::handlers::connections::test,
        
        // Connect endpoints
        crate::server::handlers::connect::connect,
        crate::server::handlers::connect::connect_ac_resume,
        crate::server::handlers::connect::connect_ac_status,
        crate::server::handlers::connect::connect_device_code,
        
        // Task endpoints
        crate::server::handlers::tasks::list,
        crate::server::handlers::tasks::create,
        crate::server::handlers::tasks::get,
        crate::server::handlers::tasks::update,
        crate::server::handlers::tasks::del,
        
        // Execute endpoints
        crate::server::handlers::execute::execute,
        crate::server::handlers::execute::execute_adhoc,
        
        // System endpoints
        crate::server::handlers::system::stats,
        crate::server::handlers::system::health,
        crate::server::handlers::system::cleanup,
        
        // AuthFlow endpoints
        crate::server::authflow::handlers::workflows::list_workflows,
        crate::server::authflow::handlers::workflows::create_workflow,
        crate::server::authflow::handlers::workflows::get_workflow,
        crate::server::authflow::handlers::workflows::get_workflow_graph,
        crate::server::authflow::handlers::workflows::validate_workflow,
        crate::server::authflow::handlers::executions::list_executions,
        crate::server::authflow::handlers::executions::get_execution,
        crate::server::authflow::handlers::executions::start_execution,
        crate::server::authflow::handlers::executions::resume_execution,
        crate::server::authflow::handlers::executions::cancel_execution,
        crate::server::authflow::handlers::executions::get_execution_trace,
        crate::server::authflow::handlers::health::health_check,
        crate::server::authflow::handlers::oauth::oauth_callback,
        crate::server::authflow::handlers::ws::websocket_handler,
    ),
    info(
        title = "OpenAct API",
        version = "0.1.0",
        description = "OpenAct - Open Authentication & Action Orchestration Platform",
        contact(
            name = "OpenAct Team",
            url = "https://github.com/aionixone/openact"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    servers(
        (url = "http://localhost:3000/api/v1", description = "Local development server"),
        (url = "/api/v1", description = "Relative path for current server")
    ),
    tags(
        (name = "connections", description = "Connection management operations"),
        (name = "tasks", description = "Task configuration and management"),
        (name = "execution", description = "Task execution and ad-hoc operations"),
        (name = "oauth", description = "OAuth 2.0 authentication flows"),
        (name = "connect", description = "One-click connection wizard"),
        (name = "system", description = "System information and health checks"),
        (name = "templates", description = "Provider templates and instantiation"),
        (name = "authflow", description = "AuthFlow workflow and execution management")
    ),
    modifiers(&SecurityAddon),
    security(
        // 默认要求 Bearer Token 认证
        ("bearer_auth" = []),
        // 或者 API Key 认证
        ("api_key" = [])
    ),
    components(
        schemas(
            // Core DTOs
            crate::interface::dto::ConnectionUpsertRequest,
            crate::interface::dto::TaskUpsertRequest,
            crate::interface::dto::AdhocExecuteRequestDto,
            crate::interface::dto::ConnectionStatusDto,
            crate::interface::dto::ExecuteOverridesDto,
            crate::interface::dto::ExecuteRequestDto,
            crate::interface::dto::ExecuteResponseDto,
            crate::interface::dto::ListQueryDto,
            
            // Connect DTOs
            crate::server::handlers::connect::ConnectMode,
            crate::server::handlers::connect::ConnectRequest,
            crate::server::handlers::connect::ConnectAcStartResponse,
            crate::server::handlers::connect::ConnectResult,
            crate::server::handlers::connect::ConnectAcResumeRequest,
            crate::server::handlers::connect::AcStatusQuery,
            crate::server::handlers::connect::DeviceCodeRequest,
            crate::server::handlers::connect::DeviceCodeResponse,
            
            // Connection Handler DTOs
            crate::server::handlers::connections::ListQuery,
            crate::server::handlers::connections::ConnectionTestRequest,
            
            // Task Handler DTOs
            crate::server::handlers::tasks::ListQuery,
            
            // AuthFlow DTOs
            crate::server::authflow::dto::CreateWorkflowRequest,
            crate::server::authflow::dto::StartExecutionRequest,
            crate::server::authflow::dto::ResumeExecutionRequest,
            crate::server::authflow::handlers::oauth::CallbackParams,
            
            // AuthFlow Response DTOs
            crate::server::authflow::dto::WorkflowSummary,
            crate::server::authflow::dto::WorkflowListResponse,
            crate::server::authflow::dto::WorkflowDetail,
            crate::server::authflow::dto::WorkflowGraphResponse,
            crate::server::authflow::dto::ValidationResult,
            crate::server::authflow::dto::ExecutionSummary,
            crate::server::authflow::dto::ExecutionListResponse,
            crate::server::authflow::dto::ExecutionDetail,
            crate::server::authflow::dto::ExecutionTraceResponse,
            crate::server::authflow::dto::ExecutionCreatedResponse,
            crate::server::authflow::dto::ExecutionActionResponse,
            crate::server::authflow::dto::StateHistoryEntry,
            
            // AuthFlow State Types
            crate::server::authflow::state::WorkflowStatus,
            crate::server::authflow::state::ExecutionStatus,
            
            // Core Models
            crate::models::connection::ConnectionConfig,
            crate::models::task::TaskConfig,
            crate::models::connection::AuthorizationType,
            crate::models::connection::AuthParameters,
            crate::models::connection::ApiKeyAuthParameters,
            crate::models::connection::BasicAuthParameters,
            crate::models::connection::OAuth2Parameters,
            crate::models::connection::InvocationHttpParameters,
            crate::models::auth::AuthConnection,
            
            // Common Types
            crate::models::common::HttpParameter,
            crate::models::common::TimeoutConfig,
            crate::models::common::TlsConfig,
            crate::models::common::NetworkConfig,
            crate::models::common::HttpPolicy,
            crate::models::common::ResponsePolicy,
            crate::models::common::RetryPolicy,
            
            // Error Types
            crate::interface::error::ApiError,
            
            // System Handler DTOs
            crate::server::handlers::system::SystemStatsResponse,
            crate::server::handlers::system::ClientPoolStats,
            crate::server::handlers::system::MemoryStats,
            crate::server::handlers::system::VersionInfo,
            crate::server::handlers::system::SystemInfo,
            crate::server::handlers::system::HealthResponse,
            crate::server::handlers::system::HealthComponents,
            crate::server::handlers::system::ComponentHealth,
            crate::server::handlers::system::CleanupResponse,
        )
    )
)]
pub struct ApiDoc;

/// Security schemes modifier for OpenAPI
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{SecurityScheme, ApiKey, ApiKeyValue, HttpAuthScheme, Http, OAuth2, Flow, ClientCredentials, Scopes};
        use std::collections::BTreeMap;
        
        let mut security_schemes = BTreeMap::new();
        
        // API Key authentication (header)
        security_schemes.insert(
            "api_key".to_string(),
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key")))
        );
        
        // Bearer token authentication (JWT/OAuth2)
        security_schemes.insert(
            "bearer_auth".to_string(),
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer))
        );
        
        // OAuth2 Client Credentials flow
        let scopes = Scopes::new();
        security_schemes.insert(
            "oauth2_client_credentials".to_string(),
            SecurityScheme::OAuth2(
                OAuth2::new([
                    Flow::ClientCredentials(
                        ClientCredentials::new("/oauth/token", scopes)
                    )
                ])
            )
        );
        
        // 最小实现：直接设置安全方案，避免复杂的合并逻辑
        
        // 创建安全方案集合
        let mut schemes = BTreeMap::new();
        schemes.insert("api_key".to_string(), SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))));
        schemes.insert("bearer_auth".to_string(), SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)));
        
        // 直接设置到 OpenAPI，让现有的 schemas 保持不变
        if openapi.components.is_none() {
            openapi.components = Some(utoipa::openapi::Components::default());
        }
        if let Some(components) = &mut openapi.components {
            // 仅设置安全方案，不影响现有 schemas
            use std::mem;
            let mut new_components = utoipa::openapi::Components::default();
            new_components.schemas = mem::take(&mut components.schemas);
            new_components.security_schemes = schemes;
            *components = new_components;
        }
    }
}


/// Generate the complete OpenAPI specification as JSON
pub fn generate_openapi_spec() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_else(|err| {
        eprintln!("Failed to generate OpenAPI spec: {}", err);
        "{}".to_string()
    })
}

/// Get the OpenAPI specification object
pub fn get_openapi_spec() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_generation() {
        let spec = get_openapi_spec();
        
        // Basic validation
        assert_eq!(spec.info.title, "OpenAct API");
        assert_eq!(spec.info.version, "0.1.0");
        
        // Check that we have the expected tags
        let tags = spec.tags.unwrap_or_default();
        let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(tag_names.contains(&"connections"));
        assert!(tag_names.contains(&"tasks"));
        assert!(tag_names.contains(&"execution"));
        assert!(tag_names.contains(&"oauth"));
        assert!(tag_names.contains(&"connect"));
        assert!(tag_names.contains(&"system"));
        assert!(tag_names.contains(&"templates"));
        
        // Check that security schemes are defined
        if let Some(components) = &spec.components {
            let security_schemes = &components.security_schemes;
            assert!(security_schemes.contains_key("api_key"));
            assert!(security_schemes.contains_key("bearer_auth"));
        }
    }

    #[test]
    fn test_openapi_json_generation() {
        let json = generate_openapi_spec();
        assert!(!json.is_empty());
        assert!(json.contains("OpenAct API"));
        
        // Validate it's proper JSON
        let _: serde_json::Value = serde_json::from_str(&json)
            .expect("Generated OpenAPI spec should be valid JSON");
    }
}
