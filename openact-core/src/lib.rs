//! OpenAct Core (minimal) - bindings only

pub mod action_registry;
pub mod auth_manager;
pub mod auth_orchestrator;
pub mod binding;
pub mod config;
pub mod database;

pub use auth_manager::AuthManager;
pub use auth_orchestrator::AuthOrchestrator;
pub use binding::{Binding, BindingManager};
pub use config::{CoreConfig, CoreContext};
pub use database::CoreDatabase;
use manifest::action::{Action, ActionExecutionContext, ActionParser, ActionParsingOptions};
pub use manifest::action::{ActionRunner, AuthAdapter};
use manifest::storage::execution_repository::ExecutionRepository;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum CoreError {
        #[error("Database: {0}")]
        Database(#[from] sqlx::Error),
        #[error("Invalid input: {0}")]
        InvalidInput(String),
    }

    pub type Result<T> = std::result::Result<T, CoreError>;
}

/// OpenAct Core (minimal) — execution orchestration
pub struct ExecutionEngine {
    pub db: CoreDatabase,
    pub bindings: BindingManager,
    pub action_registry: crate::action_registry::ActionRegistry,
    pub execution_repo: ExecutionRepository,
}

/// Optional inputs to parameterize an action execution
#[derive(Debug, Clone, Default)]
pub struct ActionInput {
    pub path_params: Option<HashMap<String, serde_json::Value>>, // { "owner": "octocat" }
    pub query: Option<HashMap<String, serde_json::Value>>,       // { "per_page": 10 }
    pub headers: Option<HashMap<String, String>>,                // { "X-Trace": "on" }
    pub body: Option<serde_json::Value>,                         // JSON body
    pub pagination: Option<PaginationOptions>,                   // controls auto pagination
}

#[derive(Debug, Clone, Default)]
pub struct PaginationOptions {
    pub all_pages: bool,
    pub max_pages: Option<u64>,
    pub per_page: Option<u64>,
}

impl ExecutionEngine {
    pub fn new(db: CoreDatabase) -> Self {
        let bindings = BindingManager::new(db.pool().clone());
        let action_registry = crate::action_registry::ActionRegistry::new(db.pool().clone());
        let execution_repo = ExecutionRepository::new(db.pool().clone());
        Self {
            db,
            bindings,
            action_registry,
            execution_repo,
        }
    }

    /// Convert stored action (DB model) to runtime action (execution model)
    fn convert_stored_to_runtime_action(
        stored_action: &manifest::storage::action_models::Action,
    ) -> error::Result<Action> {
        // Parse the OpenAPI spec to create runtime Actions
        let mut parser = ActionParser::new(ActionParsingOptions {
            default_tenant: stored_action.tenant.clone(),
            default_provider: stored_action.provider.clone(),
            ..Default::default()
        });

        // Stored spec may be YAML or JSON; serde_yaml can read both
        let spec: manifest::spec::api_spec::OpenApi30Spec =
            serde_yaml::from_str(&stored_action.openapi_spec).map_err(|e| {
                error::CoreError::InvalidInput(format!(
                    "failed to parse OpenAPI spec: {} (Only OpenAPI 3.0 is supported)",
                    e
                ))
            })?;

        let mut result = parser.parse_spec(&spec).map_err(|e| {
            error::CoreError::InvalidInput(format!("failed to parse actions from spec: {}", e))
        })?;

        // Heuristics to select the intended action and align identifiers
        if result.actions.len() == 1 {
            let mut a = result.actions.remove(0);
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        if let Some(mut a) = result
            .actions
            .iter()
            .cloned()
            .find(|a| a.name == stored_action.name)
        {
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        // Fallback: if at least one action was parsed, use the first (clone)
        if let Some(mut a) = result.actions.first().cloned() {
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        // Last resort: build action from the first path+method in the OpenAPI doc
        for (p, item) in &spec.paths.paths {
            let choose = |method: &str| -> Action {
                let op_name = item
                    .get
                    .as_ref()
                    .and_then(|op| op.operation_id.clone())
                    .unwrap_or_else(|| {
                        let segs: Vec<&str> = p
                            .trim_start_matches('/')
                            .split('/')
                            .filter(|s| !s.is_empty())
                            .collect();
                        let mut name_parts = vec![method.to_lowercase()];
                        name_parts.extend(
                            segs.into_iter()
                                .map(|s| s.trim_matches('{').trim_matches('}').to_lowercase()),
                        );
                        name_parts.join(".")
                    });
                let a = Action::new(
                    op_name,
                    method.to_string(),
                    p.clone(),
                    stored_action.provider.clone(),
                    stored_action.tenant.clone(),
                    stored_action.trn.clone(),
                );
                a
            };
            if item.get.is_some() {
                return Ok(choose("GET"));
            }
            if item.post.is_some() {
                return Ok(choose("POST"));
            }
            if item.put.is_some() {
                return Ok(choose("PUT"));
            }
            if item.delete.is_some() {
                return Ok(choose("DELETE"));
            }
            if item.patch.is_some() {
                return Ok(choose("PATCH"));
            }
            if item.head.is_some() {
                return Ok(choose("HEAD"));
            }
            if item.options.is_some() {
                return Ok(choose("OPTIONS"));
            }
            if item.trace.is_some() {
                return Ok(choose("TRACE"));
            }
        }

        Err(error::CoreError::InvalidInput(format!(
            "no actions parsed from spec for trn={}",
            stored_action.trn
        )))
    }

    /// Run action by TRN with execution persistence
    pub async fn run_action_by_trn(
        &self,
        tenant: &str,
        action_trn: &str,
        exec_trn: &str,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        self.run_action_by_trn_with_input(tenant, action_trn, exec_trn, None)
            .await
    }

    /// Run action by TRN with optional input data and execution persistence
    pub async fn run_action_by_trn_with_input(
        &self,
        tenant: &str,
        action_trn: &str,
        exec_trn: &str,
        input: Option<ActionInput>,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        use manifest::storage::action_models::{CreateExecutionRequest, ExecutionResult};

        // Ensure execution table exists
        self.execution_repo
            .ensure_table_exists()
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;

        // Fetch and validate action BEFORE creating execution record to avoid FK failures
        let stored_action = self.action_registry.get_by_trn(action_trn).await?;
        if stored_action.tenant != tenant {
            return Err(error::CoreError::InvalidInput(format!(
                "tenant mismatch: requested={}, action tenant={}",
                tenant, stored_action.tenant
            )));
        }

        // Convert to runtime action
        let runtime_action = Self::convert_stored_to_runtime_action(&stored_action)?;

        // Validate and normalize input against action definition (fill defaults, check required)
        let mut validated_input = input.unwrap_or_default();
        Self::validate_and_normalize_inputs(&runtime_action, &mut validated_input)?;

        // Create execution record (status defaults to pending in repo)
        let create_req = CreateExecutionRequest {
            execution_trn: exec_trn.to_string(),
            action_trn: action_trn.to_string(),
            tenant: tenant.to_string(),
            input_data: serde_json::to_string(&serde_json::json!({
                "path_params": validated_input.path_params,
                "query": validated_input.query,
                "headers": validated_input.headers,
                "body": validated_input.body,
            }))
            .ok(),
        };

        let execution_record = self
            .execution_repo
            .create_execution(create_req)
            .await
            .map_err(|e| {
                error::CoreError::InvalidInput(format!("failed to create execution: {}", e))
            })?;

        let start_time = std::time::Instant::now();

        // Run the action using existing run_action method or with input
        let result = {
            let inp = validated_input;
            // Build adapter (auth) and context with inputs
            // Build adapter (auth) and context with inputs
            let mut action = runtime_action;
            let mut adapter = AuthAdapter::new(tenant.to_string());
            if let Ok(db_url) = std::env::var("OPENACT_DATABASE_URL")
                .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
            {
                let _ = adapter.init_store_sqlite(db_url, true).await;
            }
            let mut runner = ActionRunner::new();
            runner.set_auth_adapter(Arc::new(adapter));
            let mut ctx = ActionExecutionContext::new(
                action.trn.clone(),
                exec_trn.to_string(),
                tenant.to_string(),
                action.provider.clone(),
            );
            if let Some(h) = inp.headers.clone() {
                for (k, v) in h {
                    ctx.headers.insert(k, v);
                }
            }
            if let Some(b) = inp.body.clone() {
                ctx.set_request_body(b);
            }
            if let Some(pp) = inp.path_params.clone() {
                for (k, v) in pp {
                    ctx.add_parameter(k, v);
                }
            }
            if let Some(q) = inp.query.clone() {
                for (k, v) in q {
                    ctx.add_parameter(k, v);
                }
            }

            // Ensure auth binding is set
            let auth_trn_opt = self
                .bindings
                .get_auth_trn_for_action(tenant, &action.trn)
                .await?;
            let auth_trn = auth_trn_opt
                .ok_or_else(|| error::CoreError::InvalidInput("No binding for action".into()))?;
            if let Some(cfg) = &mut action.auth_config {
                cfg.connection_trn = auth_trn;
            } else {
                action.auth_config = Some(manifest::action::auth::AuthConfig {
                    connection_trn: auth_trn,
                    scheme: Some("oauth2".to_string()),
                    injection: manifest::action::auth::InjectionConfig {
                        r#type: "jsonada".to_string(),
                        mapping: "{}".to_string(),
                    },
                    expiry: None,
                    refresh: None,
                    failure: None,
                });
            }

            // Build retry policy from action extensions (x-retry)
            let retry_policy = Self::build_retry_policy(&action);

            // If pagination requested and supported, iterate pages (core handles only mode=page)
            if let (Some(pcfg), Some(popt)) = (&action.pagination, &inp.pagination) {
                if popt.all_pages && pcfg.mode.to_ascii_lowercase() == "page" {
                    let mut combined: Vec<Value> = Vec::new();
                    let mut page_count: u64 = 0;
                    let mut next_page: u64 = {
                        // initial page from query or default 1
                        if let Some(q) = &inp.query {
                            if let Some(v) = q.get("page").and_then(|v| v.as_u64()) { v } else { 1 }
                        } else { 1 }
                    };
                    let per_page = popt.per_page.or_else(|| inp.query.as_ref().and_then(|q| q.get("per_page").and_then(|v| v.as_u64())));
                    let page_param = pcfg.param.clone(); // e.g., "page"
                    let max_pages = popt.max_pages.unwrap_or(100);

                    loop {
                        // clone base context for this page
                        let mut ctx_i = ctx.clone();
                        let mut page_query: HashMap<String, Value> = inp.query.clone().unwrap_or_default();
                        page_query.insert(page_param.clone(), Value::Number(serde_json::Number::from(next_page)));
                        if let Some(pp) = per_page { page_query.insert("per_page".to_string(), Value::Number(serde_json::Number::from(pp))); }
                        for (k,v) in page_query { ctx_i.add_parameter(k, v); }

                        let res = self.execute_with_retry(&action, &mut runner, ctx_i, &retry_policy, exec_trn, execution_record.id.unwrap_or(0)).await?;
                        // collect items
                        let mut fetched = 0usize;
                        if let Some(val) = &res.response_data {
                            match val {
                                Value::Array(arr) => { fetched = arr.len(); combined.extend(arr.clone()); }
                                Value::Object(obj) => {
                                    if let Some(Value::Array(items)) = obj.get("items") { fetched = items.len(); combined.extend(items.clone()); }
                                }
                                _ => {}
                            }
                        }
                        page_count += 1;
                        if fetched == 0 { break; }
                        if let Some(pp) = per_page { if fetched < pp as usize { break; } }
                        if page_count >= max_pages { break; }
                        next_page += 1;
                    }
                    // Build final result using combined items
                    return Ok(manifest::action::models::ActionExecutionResult {
                        execution_trn: exec_trn.to_string(),
                        status: manifest::action::models::ExecutionStatus::Success,
                        response_data: Some(Value::Array(combined)),
                        status_code: Some(200),
                        error_message: None,
                        duration_ms: Some(start_time.elapsed().as_millis() as u64),
                    });
                }
            }

            // Single-call (no pagination or non-page mode is delegated to ActionRunner internals)
            self.execute_with_retry(
                &action,
                &mut runner,
                ctx,
                &retry_policy,
                exec_trn,
                execution_record.id.unwrap_or(0),
            )
            .await
        };

        // Update execution record with results
        let duration = start_time.elapsed();
        match result {
            Ok(success_result) => {
                // Persist success result
                let output_data = success_result
                    .response_data
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok());
                let exec_result = ExecutionResult {
                    output_data,
                    status: "completed".to_string(),
                    status_code: success_result.status_code,
                    error_message: None,
                    duration_ms: success_result.duration_ms.map(|v| v as i64),
                };
                self.execution_repo
                    .update_execution_result(execution_record.id.unwrap_or(0), exec_result)
                    .await
                    .map_err(|e| {
                        error::CoreError::InvalidInput(format!("failed to update execution: {}", e))
                    })?;

                Ok(success_result)
            }
            Err(err) => {
                // Persist failed result
                let exec_result = ExecutionResult {
                    output_data: None,
                    status: "failed".to_string(),
                    status_code: Some(500),
                    error_message: Some(err.to_string()),
                    duration_ms: Some(duration.as_millis() as i64),
                };
                self.execution_repo
                    .update_execution_result(execution_record.id.unwrap_or(0), exec_result)
                    .await
                    .map_err(|e| {
                        error::CoreError::InvalidInput(format!("failed to update execution: {}", e))
                    })?;

                Err(err)
            }
        }
    }

    /// Run a provided Action after wiring auth via bindings
    pub async fn run_action(
        &self,
        tenant: &str,
        mut action: Action,
        execution_trn: &str,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        // Resolve binding → auth_trn
        let auth_trn_opt = self
            .bindings
            .get_auth_trn_for_action(tenant, &action.trn)
            .await?;
        let auth_trn = auth_trn_opt
            .ok_or_else(|| error::CoreError::InvalidInput("No binding for action".into()))?;

        // Ensure action has auth_config with connection_trn
        if let Some(cfg) = &mut action.auth_config {
            cfg.connection_trn = auth_trn;
        } else {
            action.auth_config = Some(manifest::action::auth::AuthConfig {
                connection_trn: auth_trn,
                scheme: Some("oauth2".to_string()),
                injection: manifest::action::auth::InjectionConfig {
                    r#type: "jsonada".to_string(),
                    mapping: "{}".to_string(),
                },
                expiry: None,
                refresh: None,
                failure: None,
            });
        }

        // Build adapter (fallback to mock if store not initialized)
        let mut adapter = AuthAdapter::new(tenant.to_string());
        if let Ok(db_url) =
            std::env::var("OPENACT_DATABASE_URL").or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
        {
            // Try to init sqlite store; ignore error to allow mock fallback
            let _ = adapter.init_store_sqlite(db_url, true).await;
        }

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(Arc::new(adapter));
        let ctx = ActionExecutionContext::new(
            action.trn.clone(),
            execution_trn.to_string(),
            tenant.to_string(),
            action.provider.clone(),
        );
        let res = runner
            .execute_action(&action, ctx)
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;
        Ok(res)
    }

    /// Run with optional overrides (timeout_ms, extra headers)
    pub async fn run_action_with_overrides(
        &self,
        tenant: &str,
        mut action: Action,
        execution_trn: &str,
        timeout_ms: Option<u64>,
        extra_headers: Option<std::collections::HashMap<String, String>>,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        if let Some(t) = timeout_ms {
            action.timeout_ms = Some(t);
        }
        // Build adapter and context with headers
        // Resolve binding → auth_trn
        let auth_trn_opt = self
            .bindings
            .get_auth_trn_for_action(tenant, &action.trn)
            .await?;
        let auth_trn = auth_trn_opt
            .ok_or_else(|| error::CoreError::InvalidInput("No binding for action".into()))?;

        if let Some(cfg) = &mut action.auth_config {
            cfg.connection_trn = auth_trn;
        } else {
            action.auth_config = Some(manifest::action::auth::AuthConfig {
                connection_trn: auth_trn,
                scheme: Some("oauth2".to_string()),
                injection: manifest::action::auth::InjectionConfig {
                    r#type: "jsonada".to_string(),
                    mapping: "{}".to_string(),
                },
                expiry: None,
                refresh: None,
                failure: None,
            });
        }

        let mut adapter = AuthAdapter::new(tenant.to_string());
        if let Ok(db_url) =
            std::env::var("OPENACT_DATABASE_URL").or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
        {
            let _ = adapter.init_store_sqlite(db_url, true).await;
        }
        let mut ctx = ActionExecutionContext::new(
            action.trn.clone(),
            execution_trn.to_string(),
            tenant.to_string(),
            action.provider.clone(),
        );
        if let Some(h) = extra_headers {
            for (k, v) in h {
                ctx.headers.insert(k, v);
            }
        }
        let default_ua = std::env::var("OPENACT_DEFAULT_USER_AGENT")
            .unwrap_or_else(|_| "openact-cli/1.0".to_string());
        ctx.headers
            .entry("User-Agent".to_string())
            .or_insert(default_ua);

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(Arc::new(adapter));
        let res = runner
            .execute_action(&action, ctx)
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;
        Ok(res)
    }

    fn extract_param_default(schema: &Value) -> Option<Value> {
        schema.get("default").cloned()
    }

    fn extract_param_type(schema: &Value) -> Option<String> {
        schema.get("type").and_then(|t| t.as_str()).map(|s| s.to_string())
    }

    fn extract_param_enum(schema: &Value) -> Option<Vec<Value>> {
        schema.get("enum").and_then(|e| e.as_array()).map(|arr| arr.clone())
    }

    fn extract_param_format(schema: &Value) -> Option<String> {
        schema.get("format").and_then(|t| t.as_str()).map(|s| s.to_string())
    }

    fn validate_format(value: &Value, fmt: &str) -> bool {
        match (fmt, value) {
            ("email", Value::String(s)) => s.contains('@'),
            ("uuid", Value::String(s)) => {
                let len = s.len();
                len == 36 && s.chars().filter(|&c| c == '-').count() == 4
            }
            ("uri", Value::String(s)) => url::Url::parse(s).is_ok(),
            ("date", Value::String(s)) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok(),
            ("date-time", Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s).is_ok(),
            _ => true,
        }
    }

    fn coerce_value_to_type(val: &Value, typ: &str) -> Option<Value> {
        match (typ, val) {
            ("string", Value::String(_)) => Some(val.clone()),
            ("string", other) => Some(Value::String(other.to_string())),
            ("integer", Value::Number(n)) if n.is_i64() || n.is_u64() => Some(val.clone()),
            ("integer", Value::String(s)) => s.parse::<i64>().ok().map(|i| Value::Number(i.into())),
            ("number", Value::Number(_)) => Some(val.clone()),
            ("number", Value::String(s)) => s
                .parse::<f64>()
                .ok()
                .and_then(|f| serde_json::Number::from_f64(f))
                .map(Value::Number),
            ("boolean", Value::Bool(_)) => Some(val.clone()),
            ("boolean", Value::String(s)) => match s.to_lowercase().as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            _ => Some(val.clone()),
        }
    }

    fn validate_and_normalize_inputs(
        action: &Action,
        input: &mut ActionInput,
    ) -> error::Result<()> {
        // Prepare maps
        let mut path_map = input.path_params.clone().unwrap_or_default();
        let mut query_map = input.query.clone().unwrap_or_default();

        // Validate path/query parameters
        for p in &action.parameters {
            let target = match p.location {
                manifest::action::models::ParameterLocation::Path => Some(&mut path_map),
                manifest::action::models::ParameterLocation::Query => Some(&mut query_map),
                _ => None,
            };
            if let Some(map_ref) = target {
                let has = map_ref.get(&p.name).cloned();
                if has.is_none() {
                    if let Some(schema) = &p.schema {
                        if let Some(defv) = Self::extract_param_default(schema) {
                            map_ref.insert(p.name.clone(), defv);
                            continue;
                        }
                    }
                    if p.required {
                        return Err(error::CoreError::InvalidInput(format!(
                            "missing required parameter '{}' in {}",
                            p.name, p.location
                        )));
                    }
                } else if let Some(schema) = &p.schema {
                    if let Some(typ) = Self::extract_param_type(schema) {
                        if let Some(coerced) = Self::coerce_value_to_type(&has.unwrap(), &typ) {
                            // enum constraint
                            if let Some(enm) = Self::extract_param_enum(schema) {
                                if !enm.iter().any(|v| v == &coerced) {
                                    return Err(error::CoreError::InvalidInput(format!("invalid value for parameter '{}' not in enum", p.name)));
                                }
                            }
                            // format constraint
                            if let Some(fmt) = Self::extract_param_format(schema) {
                                if !Self::validate_format(&coerced, &fmt) {
                                    return Err(error::CoreError::InvalidInput(format!("invalid format for parameter '{}' expected {}", p.name, fmt)));
                                }
                            }
                            map_ref.insert(p.name.clone(), coerced);
                        } else {
                            return Err(error::CoreError::InvalidInput(format!(
                                "invalid type for parameter '{}' expected {}",
                                p.name, typ
                            )));
                        }
                    }
                }
            }
        }

        input.path_params = if path_map.is_empty() {
            None
        } else {
            Some(path_map)
        };
        input.query = if query_map.is_empty() {
            None
        } else {
            Some(query_map)
        };

        // Validate request body (only application/json minimal checks)
        if let Some(rb) = &action.request_body {
            if rb.required && input.body.is_none() {
                return Err(error::CoreError::InvalidInput(
                    "missing required request body".to_string(),
                ));
            }
            if let Some(body_val) = &input.body {
                if let Some(ac) = rb.content.get("application/json") {
                    if let Some(schema) = &ac.schema {
                        if let (Some(reql), Some(props)) = (
                            schema.get("required").and_then(|x| x.as_array()),
                            schema.get("properties").and_then(|x| x.as_object()),
                        ) {
                            if let Value::Object(obj) = body_val {
                                for r in reql {
                                    if let Some(req_name) = r.as_str() {
                                        if !obj.contains_key(req_name) {
                                            return Err(error::CoreError::InvalidInput(format!(
                                                "missing required body field '{}'",
                                                req_name
                                            )));
                                        }
                                    }
                                }
                                // type/enum/format checks
                                for (k, v) in obj {
                                    if let Some(ps) = props.get(k) {
                                        if let Some(typ) = Self::extract_param_type(ps) {
                                            if Self::coerce_value_to_type(v, &typ).is_none() {
                                                return Err(error::CoreError::InvalidInput(format!("invalid type for body field '{}' expected {}", k, typ)));
                                            }
                                        }
                                        if let Some(enm) = Self::extract_param_enum(ps) {
                                            if !enm.iter().any(|ev| ev == v) {
                                                return Err(error::CoreError::InvalidInput(format!("invalid value for body field '{}' not in enum", k)));
                                            }
                                        }
                                        if let Some(fmt) = Self::extract_param_format(ps) {
                                            if !Self::validate_format(v, &fmt) {
                                                return Err(error::CoreError::InvalidInput(format!("invalid format for body field '{}' expected {}", k, fmt)));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn build_retry_policy(action: &Action) -> RetryPolicy {
        let mut policy = RetryPolicy {
            max_retries: 0,
            backoff_base_ms: 200,
            backoff_factor: 2.0,
            max_backoff_ms: 5000,
            retry_on_5xx: true,
            retry_on_429: true,
            retry_on_connect: true,
            retry_on_timeout: true,
        };
        if let Some(v) = action.extensions.get("x-retry") {
            if let Some(m) = v.as_object() {
                if let Some(n) = m.get("max_retries").and_then(|x| x.as_u64()) {
                    policy.max_retries = n as u32;
                }
                if let Some(n) = m.get("backoff_ms").and_then(|x| x.as_u64()) {
                    policy.backoff_base_ms = n as u64;
                }
                if let Some(f) = m.get("backoff_factor").and_then(|x| x.as_f64()) {
                    policy.backoff_factor = f;
                }
                if let Some(n) = m.get("max_backoff_ms").and_then(|x| x.as_u64()) {
                    policy.max_backoff_ms = n as u64;
                }
                if let Some(b) = m.get("retry_on").and_then(|x| x.as_array()) {
                    policy.retry_on_5xx = false;
                    policy.retry_on_429 = false;
                    policy.retry_on_connect = false;
                    policy.retry_on_timeout = false;
                    for item in b {
                        if let Some(s) = item.as_str() {
                            match s {
                                "5xx" => policy.retry_on_5xx = true,
                                "429" => policy.retry_on_429 = true,
                                "connect" => policy.retry_on_connect = true,
                                "timeout" => policy.retry_on_timeout = true,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        policy
    }

    async fn execute_with_retry(
        &self,
        action: &Action,
        runner: &mut ActionRunner,
        ctx: ActionExecutionContext,
        policy: &RetryPolicy,
        _exec_trn: &str,
        execution_id: i64,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        let mut attempt: u32 = 0;
        let mut delay_ms: u64 = policy.backoff_base_ms;
        #[allow(unused_assignments)]
        let mut last_class: Option<&str> = None;
        let mut last_status: Option<i32> = None;
        loop {
            let res = runner.execute_action(action, ctx.clone()).await;
            match res {
                Ok(r) => {
                    // Successful result or non-retryable status
                    if let Some(sc) = r.status_code {
                        let is_5xx = sc >= 500;
                        let is_429 = sc == 429;
                        last_status = Some(sc);
                        if (is_5xx && policy.retry_on_5xx) || (is_429 && policy.retry_on_429) {
                            last_class = Some(if is_5xx { "5xx" } else { "429" });
                            // continue to retry branch
                        } else {
                            return Ok(r);
                        }
                    } else {
                        return Ok(r);
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    let is_timeout = msg.to_lowercase().contains("timeout");
                    let is_connect = msg.to_lowercase().contains("connection")
                        || msg.to_lowercase().contains("dns");
                    if !(is_timeout && policy.retry_on_timeout)
                        && !(is_connect && policy.retry_on_connect)
                    {
                        return Err(error::CoreError::InvalidInput(msg));
                    }
                    last_class = Some(if is_timeout { "timeout" } else { "connect" });
                }
            }

            if attempt >= policy.max_retries {
                let detail = format!(
                    "retry_exhausted: class={} attempts={} last_status={}",
                    last_class.unwrap_or("other"),
                    attempt,
                    last_status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
                return Err(error::CoreError::InvalidInput(detail));
            }
            attempt += 1;
            // persist retry count
            let _ = self
                .execution_repo
                .increment_retry_count(execution_id)
                .await;

            let mut wait_ms = delay_ms;
            // Simple deterministic jitter 0-99ms based on execution id hash and attempt
            let mut hash: u64 = 1469598103934665603; // FNV offset
            for b in _exec_trn.as_bytes() {
                hash ^= *b as u64;
                hash = hash.wrapping_mul(1099511628211);
            }
            let jitter = ((hash ^ attempt as u64) % 100) as u64;
            wait_ms = wait_ms.saturating_add(jitter);
            if wait_ms > policy.max_backoff_ms {
                wait_ms = policy.max_backoff_ms;
            }
            sleep(Duration::from_millis(wait_ms)).await;
            delay_ms = (delay_ms as f64 * policy.backoff_factor) as u64;
            if delay_ms > policy.max_backoff_ms {
                delay_ms = policy.max_backoff_ms;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RetryPolicy {
    max_retries: u32,
    backoff_base_ms: u64,
    backoff_factor: f64,
    max_backoff_ms: u64,
    retry_on_5xx: bool,
    retry_on_429: bool,
    retry_on_connect: bool,
    retry_on_timeout: bool,
}

#[cfg(test)]
mod tests_engine {
    use super::*;

    fn build_mock_action(tenant: &str, provider: &str, base_url: &str) -> Action {
        let mut action = Action::new(
            "getGithubUser".to_string(),
            "GET".to_string(),
            "/user".to_string(),
            provider.to_string(),
            tenant.to_string(),
            format!("trn:openact:{}:action/{}/getUser@v1", tenant, provider),
        );
        action.timeout_ms = Some(2000);
        action.ok_path = Some("$status >= 200 and $status < 300".to_string());
        action.output_pick = Some("$body".to_string());
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base_url));
        action
    }

    #[tokio::test]
    async fn run_action_with_binding_and_mock_auth() {
        // Prepare DB and binding
        let db = CoreDatabase::connect("sqlite::memory:").await.unwrap();
        db.migrate_bindings().await.unwrap();
        let engine = ExecutionEngine::new(db.clone());

        let tenant = "tenant1";
        let provider = "github";
        let action = build_mock_action(tenant, provider, "https://api.github.com");
        // Bind auth to action
        engine
            .bindings
            .bind(
                tenant,
                "trn:authflow:tenant1:connection/github-mock",
                &action.trn,
                Some("test"),
            )
            .await
            .unwrap();

        let result = engine
            .run_action(
                tenant,
                action,
                "trn:stepflow:tenant1:execution:action-execution:core-1",
            )
            .await
            .unwrap();
        assert!(matches!(
            result.status,
            manifest::action::models::ExecutionStatus::Success
        ));
    }
}
