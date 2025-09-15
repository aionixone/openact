use anyhow::Result;
use openact_core::{AuthManager, BindingManager, ExecutionEngine, CoreDatabase};
use openact_core::action_registry::ActionRegistry;
use manifest::storage::execution_repository::ExecutionRepository;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::info;

use crate::rpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcError, METHOD_NOT_FOUND, INVALID_PARAMS, INTERNAL_ERROR, AUTH_ERROR};

#[derive(Deserialize, Default)]
struct PaginationParam {
    #[serde(default)]
    all_pages: bool,
    #[serde(default)]
    max_pages: Option<u64>,
    #[serde(default)]
    per_page: Option<u64>,
}

pub struct RpcHandler {
    auth_manager: AuthManager,
    action_registry: ActionRegistry,
    binding_manager: BindingManager,
    execution_engine: ExecutionEngine,
    execution_repository: ExecutionRepository,
}

impl RpcHandler {
    pub async fn new() -> Result<Self> {
        // Get database URL from environment
        let database_url = std::env::var("OPENACT_DATABASE_URL")
            .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
            .unwrap_or_else(|_| "sqlite:./data/openact.db".to_string());
        
        let db = CoreDatabase::connect(&database_url).await?;
        let auth_manager = AuthManager::from_database_url(database_url.clone()).await?;
        let action_registry = ActionRegistry::new(db.pool().clone());
        let binding_manager = BindingManager::new(db.pool().clone());
        let execution_engine = ExecutionEngine::new(db.clone());
        let execution_repository = ExecutionRepository::new(db.pool().clone());

        Ok(Self {
            auth_manager,
            action_registry,
            binding_manager,
            execution_engine,
            execution_repository,
        })
    }

    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        info!("Handling RPC method: {}", request.method);

        let result = match request.method.as_str() {
            // Health and status
            "health" => self.handle_health().await,
            "status" => self.handle_status().await,
            "doctor" => self.handle_doctor().await,

            // Authentication methods
            "auth.login" => self.handle_auth_login(request.params).await,
            "auth.pat" => self.handle_auth_pat(request.params).await,
            "auth.list" => self.handle_auth_list(request.params).await,
            "auth.get" => self.handle_auth_get(request.params).await,
            "auth.refresh" => self.handle_auth_refresh(request.params).await,
            "auth.delete" => self.handle_auth_delete(request.params).await,

            // Action methods
            "action.register" => self.handle_action_register(request.params).await,
            "action.list" => self.handle_action_list(request.params).await,
            "action.get" => self.handle_action_get(request.params).await,
            "action.update" => self.handle_action_update(request.params).await,
            "action.delete" => self.handle_action_delete(request.params).await,
            "action.export" => self.handle_action_export(request.params).await,

            // Binding methods
            "binding.create" => self.handle_binding_create(request.params).await,
            "binding.list" => self.handle_binding_list(request.params).await,
            "binding.get" => self.handle_binding_get(request.params).await,
            "binding.delete" => self.handle_binding_delete(request.params).await,

            // Execution methods
            "run" => self.handle_run(request.params).await,
            "execution.get" => self.handle_execution_get(request.params).await,
            "execution.list" => self.handle_execution_list(request.params).await,

            _ => Err(JsonRpcError {
                code: METHOD_NOT_FOUND,
                message: format!("Method '{}' not found", request.method),
                data: None,
            }),
        };

        match result {
            Ok(result) => JsonRpcResponse::success(request.id, result),
            Err(error) => JsonRpcResponse::error(request.id, error),
        }
    }

    // Helper to extract required parameter
    fn get_required_param<T>(&self, params: &Option<Value>, key: &str) -> Result<T, JsonRpcError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let params = params.as_ref().ok_or_else(|| JsonRpcError {
            code: INVALID_PARAMS,
            message: "Missing parameters".to_string(),
            data: None,
        })?;

        let value = params.get(key).ok_or_else(|| JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("Missing required parameter: {}", key),
            data: None,
        })?;

        serde_json::from_value(value.clone()).map_err(|e| JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("Invalid parameter type for '{}': {}", key, e),
            data: None,
        })
    }

    // Helper to extract optional parameter
    fn get_optional_param<T>(&self, params: &Option<Value>, key: &str) -> Result<Option<T>, JsonRpcError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let Some(params) = params.as_ref() else {
            return Ok(None);
        };

        let Some(value) = params.get(key) else {
            return Ok(None);
        };

        match serde_json::from_value(value.clone()) {
            Ok(val) => Ok(Some(val)),
            Err(e) => Err(JsonRpcError {
                code: INVALID_PARAMS,
                message: format!("Invalid parameter type for '{}': {}", key, e),
                data: None,
            }),
        }
    }

    // Health and status methods
    async fn handle_health(&self) -> Result<Value, JsonRpcError> {
        Ok(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn handle_status(&self) -> Result<Value, JsonRpcError> {
        let auth_count = self.auth_manager.count().await
            .map_err(|e| JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to get auth count: {}", e),
                data: None,
            })?;

        Ok(json!({
            "status": "running",
            "interface": "stdio-rpc", 
            "version": env!("CARGO_PKG_VERSION"),
            "stats": {
                "auth_connections": auth_count,
                "registered_actions": "N/A", 
                "bindings": "N/A"
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn handle_doctor(&self) -> Result<Value, JsonRpcError> {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();

        // Check environment variables
        if std::env::var("OPENACT_MASTER_KEY").is_err() {
            issues.push("OPENACT_MASTER_KEY not set".to_string());
            suggestions.push("Set OPENACT_MASTER_KEY environment variable for encryption".to_string());
        }

        let status = if issues.is_empty() { "healthy" } else { "issues_found" };

        Ok(json!({
            "status": status,
            "issues": issues,
            "suggestions": suggestions,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    // Authentication methods - simplified implementations
    async fn handle_auth_login(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        // For now, return a placeholder indicating this needs OAuth flow implementation
        Ok(json!({
            "message": "OAuth login via STDIO-RPC requires external callback handling",
            "suggestion": "Use CLI or HTTP interface for interactive OAuth flows"
        }))
    }

    async fn handle_auth_pat(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_required_param(&params, "tenant")?;
        let provider: String = self.get_required_param(&params, "provider")?;
        let user_id: String = self.get_required_param(&params, "user_id")?;
        let access_token: String = self.get_required_param(&params, "access_token")?;

        match self.auth_manager.create_pat_connection(&tenant, &provider, &user_id, &access_token).await {
            Ok(trn) => Ok(json!({ "connection_trn": trn })),
            Err(e) => Err(JsonRpcError {
                code: AUTH_ERROR,
                message: format!("Failed to create PAT connection: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_auth_list(&self, _params: Option<Value>) -> Result<Value, JsonRpcError> {
        match self.auth_manager.list().await {
            Ok(connection_trns) => Ok(json!({ "connections": connection_trns })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to list connections: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_auth_get(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        
        match self.auth_manager.get(&trn).await {
            Ok(Some(connection)) => Ok(json!({ "connection": connection })),
            Ok(None) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: "Connection not found".to_string(),
                data: None,
            }),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to get connection: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_auth_refresh(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        let access_token: String = self.get_required_param(&params, "access_token")?;
        let refresh_token: Option<String> = self.get_optional_param(&params, "refresh_token")?;
        let expires_in: Option<i64> = self.get_optional_param(&params, "expires_in")?;
        
        match self.auth_manager.refresh_connection(&trn, &access_token, refresh_token.as_deref(), expires_in).await {
            Ok(_) => Ok(json!({ "status": "refreshed" })),
            Err(e) => Err(JsonRpcError {
                code: AUTH_ERROR,
                message: format!("Failed to refresh connection: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_auth_delete(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        
        match self.auth_manager.delete(&trn).await {
            Ok(true) => Ok(json!({ "deleted": true })),
            Ok(false) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: "Connection not found".to_string(), 
                data: None,
            }),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to delete connection: {}", e),
                data: None,
            }),
        }
    }

    // Action methods
    async fn handle_action_register(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let config_path: String = self.get_required_param(&params, "config_path")?;
        let tenant: String = self.get_optional_param(&params, "tenant")?.unwrap_or_else(|| "default".to_string());
        let provider: String = self.get_optional_param(&params, "provider")?.unwrap_or_else(|| "unknown".to_string());
        let name: String = self.get_optional_param(&params, "name")?.unwrap_or_else(|| "action".to_string());
        
        // Generate a simple TRN for the action
        let trn = format!("trn:openact:{}:action/{}/{}@v1", tenant, provider, name);
        
        match self.action_registry.register_from_yaml(&tenant, &provider, &name, &trn, &PathBuf::from(config_path)).await {
            Ok(_) => Ok(json!({ "action_trn": trn })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to register action: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_action_list(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_optional_param(&params, "tenant")?.unwrap_or_else(|| "default".to_string());
        
        match self.action_registry.list_by_tenant(&tenant).await {
            Ok(actions) => Ok(json!({ "actions": actions })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to list actions: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_action_get(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        
        match self.action_registry.get_by_trn(&trn).await {
            Ok(action) => Ok(json!({ "action": action })),
            Err(e) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: format!("Failed to get action: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_action_update(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        let config_path: String = self.get_required_param(&params, "config_path")?;
        
        match self.action_registry.update_from_yaml(&trn, &PathBuf::from(config_path)).await {
            Ok(action) => Ok(json!({ "updated_action": action })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to update action: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_action_delete(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        
        match self.action_registry.delete_by_trn(&trn).await {
            Ok(true) => Ok(json!({ "deleted": true })),
            Ok(false) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: "Action not found".to_string(),
                data: None,
            }),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to delete action: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_action_export(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let trn: String = self.get_required_param(&params, "trn")?;
        
        match self.action_registry.export_spec_by_trn(&trn).await {
            Ok(spec) => Ok(json!({ "exported_spec": spec })),
            Err(e) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: format!("Failed to export action: {}", e),
                data: None,
            }),
        }
    }

    // Binding methods
    async fn handle_binding_create(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_required_param(&params, "tenant")?;
        let action_trn: String = self.get_required_param(&params, "action_trn")?;
        let auth_trn: String = self.get_required_param(&params, "auth_trn")?;
        
        match self.binding_manager.bind(&tenant, &auth_trn, &action_trn, Some("stdio-rpc")).await {
            Ok(_) => {
                let binding_trn = format!("trn:openact:{}:binding:{}:{}", tenant, auth_trn, action_trn);
                Ok(json!({ "binding_trn": binding_trn }))
            },
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to create binding: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_binding_list(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_optional_param(&params, "tenant")?.unwrap_or_else(|| "default".to_string());
        
        match self.binding_manager.list_by_tenant(&tenant).await {
            Ok(bindings) => Ok(json!({ "bindings": bindings })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to list bindings: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_binding_get(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_required_param(&params, "tenant")?;
        let action_trn: String = self.get_required_param(&params, "action_trn")?;
        
        match self.binding_manager.get_auth_trn_for_action(&tenant, &action_trn).await {
            Ok(Some(auth_trn)) => Ok(json!({ 
                "binding": {
                    "tenant": tenant,
                    "action_trn": action_trn,
                    "auth_trn": auth_trn
                }
            })),
            Ok(None) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: "Binding not found".to_string(),
                data: None,
            }),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to get binding: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_binding_delete(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_required_param(&params, "tenant")?;
        let action_trn: String = self.get_required_param(&params, "action_trn")?;
        let auth_trn: String = self.get_required_param(&params, "auth_trn")?;
        
        match self.binding_manager.unbind(&tenant, &auth_trn, &action_trn).await {
            Ok(true) => Ok(json!({ "deleted": true })),
            Ok(false) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: "Binding not found".to_string(),
                data: None,
            }),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to delete binding: {}", e),
                data: None,
            }),
        }
    }

    // Execution methods
    async fn handle_run(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: String = self.get_required_param(&params, "tenant")?;
        let action_trn: String = self.get_required_param(&params, "action_trn")?;
        let input_data: Option<Value> = self.get_optional_param(&params, "input_data")?;
        let dry_run: bool = self.get_optional_param(&params, "dry_run")?.unwrap_or(false);
        let pagination_param: Option<PaginationParam> = self.get_optional_param(&params, "pagination")?;

        let execution_trn = format!("trn:openact:{}:execution:{}", tenant, uuid::Uuid::new_v4());

        if dry_run {
            return Ok(json!({
                "ok": true,
                "data": {
                    "execution_trn": execution_trn,
                    "status": "dry_run",
                    "action_trn": action_trn,
                    "tenant": tenant,
                    "input_data": input_data,
                    "message": "Dry run completed - no execution created"
                }
            }));
        }

        // Build optional input and execute via core engine (engine handles persistence)
        let mut input_opt = input_data.and_then(|v| {
            let path_params = v.get("path_params").and_then(|x| x.as_object()).map(|m| m.clone());
            let query = v.get("query").and_then(|x| x.as_object()).map(|m| m.clone());
            let headers = v.get("headers").and_then(|x| x.as_object()).map(|m| m.iter().filter_map(|(k,v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect());
            let body = v.get("body").cloned();
            Some(openact_core::ActionInput {
                path_params: path_params.map(|m| m.into_iter().map(|(k,v)| (k, v)).collect()),
                query: query.map(|m| m.into_iter().map(|(k,v)| (k, v)).collect()),
                headers,
                body,
                pagination: None,
            })
        });

        if let Some(p) = pagination_param {
            let po = openact_core::PaginationOptions { all_pages: p.all_pages, max_pages: p.max_pages, per_page: p.per_page };
            input_opt = Some(match input_opt {
                Some(mut i) => { i.pagination = Some(po); i }
                None => openact_core::ActionInput { path_params: None, query: None, headers: None, body: None, pagination: Some(po) }
            });
        }

        match self
            .execution_engine
            .run_action_by_trn_with_input(&tenant, &action_trn, &execution_trn, input_opt)
            .await
        {
            Ok(exec_result) => {
                Ok(json!({
                    "ok": true,
                    "data": {
                        "execution_trn": execution_trn,
                        "status": format!("{}", match exec_result.status { manifest::action::models::ExecutionStatus::Success => "completed", manifest::action::models::ExecutionStatus::Failed => "failed", _ => "running" }),
                        "status_code": exec_result.status_code,
                        "duration_ms": exec_result.duration_ms,
                        "response_data": exec_result.response_data,
                        "error_message": exec_result.error_message,
                    }
                }))
            }
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to execute action: {}", e),
                data: Some(json!({
                    "execution_trn": execution_trn,
                    "action_trn": action_trn,
                    "tenant": tenant,
                })),
            }),
        }
    }

    async fn handle_execution_get(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let execution_trn: String = self.get_required_param(&params, "execution_trn")?;
        
        // Ensure table exists before querying
        self.execution_repository.ensure_table_exists().await
            .map_err(|e| JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to initialize execution table: {}", e),
                data: None,
            })?;
        
        match self.execution_repository.get_execution_by_trn(&execution_trn).await {
            Ok(execution) => Ok(json!({ "ok": true, "data": { "execution": execution } })),
            Err(e) => Err(JsonRpcError {
                code: -32003, // NOT_FOUND_ERROR
                message: format!("Execution not found: {}", e),
                data: None,
            }),
        }
    }

    async fn handle_execution_list(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let tenant: Option<String> = self.get_optional_param(&params, "tenant")?;
        let limit: Option<i64> = self.get_optional_param(&params, "limit")?;
        let offset: Option<i64> = self.get_optional_param(&params, "offset")?;
        
        // Ensure table exists before querying
        self.execution_repository.ensure_table_exists().await
            .map_err(|e| JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to initialize execution table: {}", e),
                data: None,
            })?;
        
        let executions = if let Some(tenant) = tenant {
            // List by tenant
            self.execution_repository.get_executions_by_tenant(&tenant, limit, offset).await
        } else {
            // If no tenant specified, default to "default" tenant
            self.execution_repository.get_executions_by_tenant("default", limit, offset).await
        };
        
        match executions {
            Ok(executions) => Ok(json!({ 
                "ok": true,
                "data": {
                    "executions": executions,
                    "count": executions.len()
                }
            })),
            Err(e) => Err(JsonRpcError {
                code: INTERNAL_ERROR,
                message: format!("Failed to list executions: {}", e),
                data: None,
            }),
        }
    }
}
