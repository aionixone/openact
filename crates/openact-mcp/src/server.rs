//! MCP server implementation following Go reference pattern

use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::{
    jsonrpc::{
        error_response, success_response, JsonRpcRequest, JsonRpcResponse, RequestId,
        JSONRPC_VERSION,
    },
    mcp::{
        ContentBlock, InitializeRequest, InitializeResponse, ServerCapabilities, Tool,
        ToolAnnotations, ToolsCallRequest, ToolsCallResponse, ToolsListRequest, ToolsListResponse,
        LATEST_PROTOCOL_VERSION, METHOD_INITIALIZE, METHOD_PING, METHOD_TOOLS_CALL,
        METHOD_TOOLS_LIST, SUPPORTED_PROTOCOL_VERSIONS,
    },
    AppState, GovernanceConfig, McpError, McpResult,
};
use openact_core::store::ActionRepository;
use openact_core::{types::ToolName, ConnectorKind, Trn};
use openact_plugins as plugins;
use openact_registry::{ConnectorRegistry, ExecutionContext};

/// MCP Server
pub struct McpServer {
    pub app_state: AppState,
    registry: ConnectorRegistry,
    governance: GovernanceConfig,
}

impl McpServer {
    pub fn new(app_state: AppState, governance: GovernanceConfig) -> Self {
        // Build a registry using the shared SqlStore for both connections and actions
        let store_arc = app_state.store.clone();
        // Pass concrete SqlStore values (traits are implemented for SqlStore, not Arc<SqlStore>)
        let conn_store = store_arc.as_ref().clone();
        let act_repo = store_arc.as_ref().clone();
        let mut registry = ConnectorRegistry::new(conn_store, act_repo);

        for registrar in plugins::registrars() {
            registrar(&mut registry);
        }

        Self { app_state, registry, governance }
    }

    /// Process a single MCP message (following Go's processMcpMessage pattern)
    pub async fn process_message(&self, body: &[u8]) -> McpResult<Option<JsonRpcResponse>> {
        // Parse the JSON-RPC request
        let request: JsonRpcRequest = serde_json::from_slice(body).map_err(|e| {
            error!("Failed to parse JSON-RPC request: {}", e);
            McpError::Serialization(e)
        })?;

        debug!("Processing method: {}", request.method);

        // Check JSON-RPC version
        if request.jsonrpc != JSONRPC_VERSION {
            return Ok(Some(error_response(
                request.id,
                crate::jsonrpc::JsonRpcError::invalid_request()
                    .with_data(serde_json::json!({"message": "Invalid JSON-RPC version"})),
            )));
        }

        // Handle notifications (no response needed)
        if request.id.is_none() {
            self.handle_notification(&request).await?;
            return Ok(None);
        }

        // Handle requests
        let response = match request.method.as_str() {
            METHOD_INITIALIZE => self.handle_initialize(&request).await?,
            METHOD_PING => self.handle_ping(&request).await?,
            METHOD_TOOLS_LIST => self.handle_tools_list(&request).await?,
            METHOD_TOOLS_CALL => self.handle_tools_call(&request).await?,
            _ => error_response(
                request.id,
                crate::jsonrpc::JsonRpcError::method_not_found()
                    .with_data(serde_json::json!({"method": request.method})),
            ),
        };

        Ok(Some(response))
    }

    /// Handle notification messages
    async fn handle_notification(&self, _request: &JsonRpcRequest) -> McpResult<()> {
        // Currently no notifications are processed
        debug!("Received notification, ignoring");
        Ok(())
    }

    /// Handle initialize method
    async fn handle_initialize(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let params = request.params.as_ref().ok_or_else(|| {
            McpError::InvalidArguments("Missing params for initialize".to_string())
        })?;

        let init_request: InitializeRequest = serde_json::from_value(params.clone())?;

        // Choose protocol version
        let protocol_version =
            if SUPPORTED_PROTOCOL_VERSIONS.contains(&init_request.protocol_version.as_str()) {
                init_request.protocol_version
            } else {
                LATEST_PROTOCOL_VERSION.to_string()
            };

        let response = InitializeResponse {
            protocol_version,
            capabilities: ServerCapabilities {
                completions: None,
                experimental: None,
                logging: None,
                prompts: None,
                resources: None,
                tools: Some(openact_mcp_types::ServerCapabilitiesTools { list_changed: None }),
            },
            server_info: openact_mcp_types::Implementation {
                name: "OpenAct MCP Server".to_string(),
                title: Some("OpenAct MCP".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                user_agent: None,
            },
            instructions: Some(
                "OpenAct MCP Server - Execute actions through connectors".to_string(),
            ),
        };

        Ok(success_response(request.id.clone(), serde_json::to_value(response)?))
    }

    /// Handle ping method
    async fn handle_ping(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        Ok(success_response(request.id.clone(), serde_json::json!({})))
    }

    /// Handle tools/list method
    async fn handle_tools_list(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let raw_params = request.params.clone();
        let _params: ToolsListRequest = if let Some(params) = &raw_params {
            serde_json::from_value(params.clone())?
        } else {
            ToolsListRequest { cursor: None }
        };
        // Optional tenant context (future-friendly): try to read unknown field `tenant`
        let tenant_ctx: Option<String> = raw_params
            .as_ref()
            .and_then(|v| v.get("tenant"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Dynamic tools from store (mcp_enabled)
        let mut tools: Vec<Tool> = Vec::new();

        // Include generic executor if allowed by governance
        let openact_execute_name = "openact.execute";
        if self.governance.is_tool_allowed(openact_execute_name) {
            tools.push(Tool {
                name: openact_execute_name.to_string(),
                description: Some("Execute an OpenAct action using either explicit TRN or connector/action components".to_string()),
                title: Some("OpenAct Execute".to_string()),
                annotations: Some(ToolAnnotations {
                    destructive_hint: Some(false),
                    idempotent_hint: None,
                    open_world_hint: None,
                    read_only_hint: Some(false),
                    title: Some("Execute OpenAct action".to_string()),
                }),
                input_schema: openact_mcp_types::ToolInputSchema {
                    r#type: "object".into(),
                    properties: Some(serde_json::json!({
                        "action_trn": {"type": "string", "description": "Explicit action TRN (e.g., 'trn:openact:tenant:action/http/get@v1')"},
                        "connector": {"type": "string", "description": "Connector type (e.g., 'http') - required when action_trn not provided"},
                        "action": {"type": "string", "description": "Action name (e.g., 'get') - required when action_trn not provided"},
                        "tenant": {"type": "string", "description": "Tenant name (default: 'default')"},
                        "version": {"type": "integer", "description": "Action version (default: latest)"},
                        "input": {"type": "object", "description": "Input parameters for the action"}
                    })),
                    required: Some(vec!["input".into()]),
                },
                output_schema: None,
            });
        } else {
            debug!("Tool '{}' filtered by governance policy", openact_execute_name);
        }

        // Optimize: Get all MCP-enabled actions in one query to avoid N+1
        let all_actions = self.get_all_mcp_enabled_actions(tenant_ctx.as_deref()).await?;
        let mut tool_names_seen = HashSet::new();
        let mut alias_conflicts = Vec::new();

        for a in all_actions {
            // Determine tool name: use mcp_overrides.tool_name if available, otherwise connector.action
            let tool_name = a
                .mcp_overrides
                .as_ref()
                .and_then(|o| o.tool_name.clone())
                .unwrap_or_else(|| format!("{}.{}", a.connector.as_str(), a.name));

            // Check for alias conflicts
            if tool_names_seen.contains(&tool_name) {
                alias_conflicts.push(tool_name.clone());
                warn!(
                    "Tool name conflict detected: '{}' (from action: {}.{})",
                    tool_name,
                    a.connector.as_str(),
                    a.name
                );
                continue; // Skip duplicate tools
            }
            tool_names_seen.insert(tool_name.clone());

            // Apply governance filtering
            if !self.governance.is_tool_allowed(&tool_name) {
                debug!("Tool '{}' filtered by governance policy", tool_name);
                continue;
            }

            // Determine description/title (prefer overrides)
            let description = a.mcp_overrides.as_ref().and_then(|o| o.description.clone());
            let title = description.clone();

            // Derive schemas via action instance MCP hooks
            let (input_schema, output_schema) = match self.registry.derive_mcp_schemas(&a).await {
                Ok((input_v, output_v_opt)) => {
                    let input =
                        serde_json::from_value::<openact_mcp_types::ToolInputSchema>(input_v)
                            .unwrap_or(openact_mcp_types::ToolInputSchema {
                                r#type: "object".into(),
                                properties: None,
                                required: None,
                            });
                    let output = output_v_opt.and_then(|v| {
                        serde_json::from_value::<openact_mcp_types::ToolOutputSchema>(v).ok()
                    });
                    (input, output)
                }
                Err(e) => {
                    warn!(
                        "Failed to derive MCP schemas for {}.{}: {}",
                        a.connector.as_str(),
                        a.name,
                        e
                    );
                    (
                        openact_mcp_types::ToolInputSchema {
                            r#type: "object".into(),
                            properties: None,
                            required: None,
                        },
                        None,
                    )
                }
            };

            // Annotations: fetch from registry hook if provided (JSON -> ToolAnnotations)
            let annotations = match self.registry.derive_mcp_annotations(&a).await {
                Ok(Some(json_val)) => serde_json::from_value::<ToolAnnotations>(json_val).ok(),
                _ => None,
            };

            tools.push(Tool {
                name: tool_name,
                description,
                title,
                annotations,
                input_schema,
                output_schema,
            });
        }

        // Log alias conflicts if any
        if !alias_conflicts.is_empty() {
            warn!("Detected {} tool name conflicts: {:?}", alias_conflicts.len(), alias_conflicts);
        }

        let response = ToolsListResponse { tools, next_cursor: None };

        Ok(success_response(request.id.clone(), serde_json::to_value(response)?))
    }

    /// Handle tools/call method
    async fn handle_tools_call(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let params = request.params.as_ref().ok_or_else(|| {
            McpError::InvalidArguments("Missing params for tools/call".to_string())
        })?;

        let call_request: ToolsCallRequest = serde_json::from_value(params.clone())?;

        debug!("Calling tool: {}", call_request.name);

        // Apply governance filtering for tool calls
        if !self.governance.is_tool_allowed(&call_request.name) {
            warn!("Tool '{}' denied by governance policy", call_request.name);
            return Err(McpError::PermissionDenied(format!(
                "Tool '{}' is not allowed",
                call_request.name
            )));
        }

        // Acquire concurrency permit
        let _permit =
            self.governance.concurrency_limiter.acquire().await.map_err(|_| {
                McpError::Internal("Failed to acquire concurrency permit".to_string())
            })?;

        debug!("Acquired concurrency permit for tool: {}", call_request.name);

        // Execute with timeout
        let execution_future = async {
            match call_request.name.as_str() {
                "openact.execute" => {
                    let empty = serde_json::json!({});
                    let args_ref = call_request.arguments.as_ref().unwrap_or(&empty);
                    let result = self.execute_openact_action(args_ref).await?;
                    Ok(success_response(request.id.clone(), serde_json::to_value(result)?))
                }
                // For per-action tools (both direct connector.action and aliased tools)
                other => {
                    let (connector, action) = self.resolve_tool_to_action(other).await?;

                    // Flatten arguments: if user provided { input: {...} } use that;
                    // otherwise, treat the whole arguments object as input. Also pass through
                    // optional tenant/version if provided in arguments.
                    let empty = serde_json::json!({});
                    let args_ref = call_request.arguments.as_ref().unwrap_or(&empty);

                    let (tenant_val, version_val) = if let Some(obj) = args_ref.as_object() {
                        (obj.get("tenant").cloned(), obj.get("version").cloned())
                    } else {
                        (None, None)
                    };

                    let input_value = if let Some(obj) = args_ref.as_object() {
                        if let Some(inner) = obj.get("input") {
                            inner.clone()
                        } else {
                            // Use full object as input, but it may contain tenant/version; keep simple for now.
                            args_ref.clone()
                        }
                    } else {
                        args_ref.clone()
                    };

                    let mut wrapped = serde_json::json!({
                        "connector": connector,
                        "action": action,
                        "input": input_value
                    });

                    if let Some(t) = tenant_val {
                        wrapped["tenant"] = t;
                    }
                    if let Some(v) = version_val {
                        wrapped["version"] = v;
                    }

                    let result = self.execute_openact_action(&wrapped).await?;
                    Ok(success_response(request.id.clone(), serde_json::to_value(result)?))
                }
            }
        };

        // Apply timeout
        let start_time = std::time::Instant::now();
        match timeout(self.governance.timeout, execution_future).await {
            Ok(result) => {
                let elapsed_ms = start_time.elapsed().as_millis() as u64;
                let tenant_log = call_request
                    .arguments
                    .as_ref()
                    .and_then(|a| a.get("tenant"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                match tenant_log {
                    Some(t) => info!(tool=%call_request.name, tenant=%t, elapsed_ms=%elapsed_ms, "MCP tools/call done"),
                    None => info!(tool=%call_request.name, elapsed_ms=%elapsed_ms, "MCP tools/call done"),
                }
                result
            }
            Err(_) => {
                warn!("Tool '{}' timed out after {:?}", call_request.name, self.governance.timeout);
                Err(McpError::Timeout)
            }
        }
    }

    /// Get all MCP-enabled actions in a single query to avoid N+1 problem
    async fn get_all_mcp_enabled_actions(
        &self,
        tenant: Option<&str>,
    ) -> McpResult<Vec<openact_core::types::ActionRecord>> {
        let mut filter = openact_core::store::ActionListFilter { mcp_enabled: Some(true), ..Default::default() };
        if let Some(t) = tenant { filter.tenant = Some(t.to_string()); }
        // Push governance to DB layer when listing actions
        filter.allow_patterns = Some(self.governance.allow_patterns.clone());
        filter.deny_patterns = Some(self.governance.deny_patterns.clone());
        let actions = ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
        .await
        .map_err(|e| McpError::Internal(format!("Failed to list MCP-enabled actions: {}", e)))?;

        Ok(actions)
    }

    /// Resolve tool name to (connector, action) pair
    async fn resolve_tool_to_action(&self, tool_name: &str) -> McpResult<(String, String)> {
        // First try to find it as an alias in mcp_overrides.tool_name using a filtered list
        let filter = openact_core::store::ActionListFilter { mcp_enabled: Some(true), ..Default::default() };
        let actions = ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
        .await
        .map_err(|e| McpError::Internal(format!("Failed to list actions: {}", e)))?;

        for a in actions {
            if let Some(ref overrides) = a.mcp_overrides {
                if let Some(ref alias) = overrides.tool_name {
                    if alias == tool_name {
                        debug!(
                            "Resolved alias '{}' to {}.{}",
                            tool_name,
                            a.connector.as_str(),
                            a.name
                        );
                        return Ok((a.connector.as_str().to_string(), a.name));
                    }
                }
            }
        }

        // If not found as alias, try direct connector.action format
        if let Some(parsed) = ToolName::parse_human(tool_name) {
            let kind = ConnectorKind::new(parsed.connector.clone()).canonical();
            let mut filter = openact_core::store::ActionListFilter { connector: Some(kind), mcp_enabled: Some(true), ..Default::default() };
            filter.allow_patterns = Some(self.governance.allow_patterns.clone());
            filter.deny_patterns = Some(self.governance.deny_patterns.clone());
            let actions = ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
            .await
            .map_err(|e| McpError::Internal(format!("Failed to list actions: {}", e)))?;
            if actions.iter().any(|a| a.name == parsed.action) {
                debug!(
                    "Resolved direct tool '{}' to {}.{}",
                    tool_name, parsed.connector, parsed.action
                );
                return Ok((parsed.connector, parsed.action));
            }
        }

        Err(McpError::ToolNotFound(format!("Tool not found: {}", tool_name)))
    }

    /// Execute an OpenAct action
    async fn execute_openact_action(&self, arguments: &Value) -> McpResult<ToolsCallResponse> {
        // Validate arguments object
        if !arguments.is_object() {
            return Err(McpError::InvalidArguments("Arguments must be an object".to_string()));
        }

        let input = arguments
            .get("input")
            .ok_or_else(|| McpError::InvalidArguments("Missing 'input' field".to_string()))?;
        let stream_requested = arguments
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Check if explicit action_trn is provided
        let action_trn = if let Some(trn_str) = arguments.get("action_trn").and_then(|v| v.as_str())
        {
            // Use explicit TRN - validate format and existence
            let trn = Trn::new(trn_str.to_string());
            let _atrn = openact_core::types::ActionTrn::try_from(trn.clone()).map_err(|_| {
                McpError::InvalidArguments("Invalid action TRN".to_string())
            })?;
            let action_record = ActionRepository::get(self.app_state.store.as_ref(), &trn)
                .await
                .map_err(|e| McpError::Internal(format!("Failed to lookup action TRN: {}", e)))?
                .ok_or_else(|| {
                    McpError::ToolNotFound(format!("Action TRN not found: {}", trn_str))
                })?;

            info!("Using explicit TRN: {}", trn_str);
            action_record.trn
        } else {
            // Parse individual components
            let connector = arguments.get("connector")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| McpError::InvalidArguments("Missing or empty 'connector' field (required when action_trn not provided)".to_string()))?;

            let action = arguments
                .get("action")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    McpError::InvalidArguments(
                        "Missing or empty 'action' field (required when action_trn not provided)"
                            .to_string(),
                    )
                })?;

            let tenant = arguments.get("tenant").and_then(|v| v.as_str()).unwrap_or("default");
            // Track whether caller provided version at all
            let has_version_param = arguments.get("version").is_some();
            // Accept version as number or string "latest". When "latest", treat as None to pick highest.
            let version_opt = match arguments.get("version") {
                Some(v) if v.is_i64() => v.as_i64(),
                Some(v) if v.is_u64() => v.as_u64().and_then(|n| i64::try_from(n).ok()),
                Some(v) if v.is_string() => match v.as_str().unwrap_or("") {
                    "latest" | "" => None,
                    s => s.parse::<i64>().ok(),
                },
                _ => None,
            };

            info!(
                "Resolving action: {}.{} (tenant={} version={:?})",
                connector, action, tenant, version_opt
            );

            // Enforce explicit version when resolving by name (not TRN)
            if !has_version_param {
                return Err(McpError::InvalidArguments(
                    openact_core::policy::messages::version_required_message().to_string(),
                ));
            }

            // Resolve action TRN via shared resolver
            let kind = ConnectorKind::new(connector.to_string()).canonical();
            let trn = openact_core::resolve::resolve_action_trn_by_name(
                self.app_state.store.as_ref(),
                tenant,
                &kind,
                action,
                version_opt,
            )
            .await
            .map_err(|e| match e {
                openact_core::CoreError::NotFound(msg) => McpError::ToolNotFound(msg),
                openact_core::CoreError::Invalid(msg) => McpError::InvalidArguments(msg),
                other => McpError::Internal(other.to_string()),
            })?;

            trn
        };
        let ctx = ExecutionContext::new();
        let start_time = std::time::Instant::now();
        let exec = self
            .registry
            .execute(&action_trn, input.clone(), Some(ctx))
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        info!(action_trn=%action_trn.as_str(), elapsed_ms=%elapsed_ms, "MCP openact.execute finished");

        // The action has already had a chance to wrap/normalize its output via
        // Action::mcp_wrap_output (applied in the registry). MCP server should
        // treat it as the canonical structured content and avoid per-connector
        // branching here.
        // Attach stream hint if requested (final-only for now; incremental MCP streaming can be added later)
        let mut structured = exec.output.clone();
        if stream_requested {
            if let serde_json::Value::Object(ref mut map) = structured {
                map.insert(
                    "_stream".to_string(),
                    serde_json::json!({"mode": "final", "note": "incremental MCP streaming not enabled; returned final frame"}),
                );
            }
        }
        let text = serde_json::to_string(&structured).unwrap_or_else(|_| "{}".to_string());

        let block = ContentBlock::TextContent(openact_mcp_types::TextContent {
            annotations: None,
            text,
            r#type: "text".into(),
        });
        Ok(ToolsCallResponse {
            content: vec![block],
            is_error: None,
            structured_content: Some(structured),
        })
    }
}

/// Serve MCP over stdio (following Go's stdio pattern)
pub async fn serve_stdio(app_state: AppState, governance: GovernanceConfig) -> McpResult<()> {
    info!("Starting OpenAct MCP server (stdio mode)");
    info!(
        "Governance: max_concurrency={}, timeout={:?}",
        governance.max_concurrency, governance.timeout
    );
    if !governance.allow_patterns.is_empty() {
        info!("Allow patterns: {:?}", governance.allow_patterns);
    }
    if !governance.deny_patterns.is_empty() {
        info!("Deny patterns: {:?}", governance.deny_patterns);
    }

    let server = McpServer::new(app_state, governance.clone());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Read lines from stdin
    for line in BufReader::new(stdin).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        debug!("Processing line: {}", line);

        // Check for batch requests (arrays) - MCP doesn't support batch
        let trimmed_line = line.trim();
        if trimmed_line.starts_with('[') {
            error!("Batch requests are not supported");
            let error_response = error_response(
                None,
                crate::jsonrpc::JsonRpcError::invalid_request()
                    .with_data(serde_json::json!({"message": "Batch requests are not supported"})),
            );
            let response_json = serde_json::to_string(&error_response)?;
            writeln!(stdout, "{}", response_json)?;
            stdout.flush()?;
            continue;
        }

        // Enforce tenant requirement for stdio if configured and optionally inject default when not required
        let require_tenant = std::env::var("OPENACT_REQUIRE_TENANT")
            .map(|v| { let v = v.to_ascii_lowercase(); v == "1" || v == "true" || v == "yes" })
            .unwrap_or(false);
        let mut maybe_patched: Option<Vec<u8>> = None;
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                // Log method/id/tenant for stdio
                let id_str = v.get("id").map(|id| id.to_string()).unwrap_or_else(|| "null".to_string());
                let tenant_log = match method {
                    crate::mcp::METHOD_TOOLS_LIST => v.get("params").and_then(|p| p.get("tenant")).and_then(|t| t.as_str()).map(|s| s.to_string()),
                    crate::mcp::METHOD_TOOLS_CALL => v.get("params").and_then(|p| p.get("arguments")).and_then(|a| a.get("tenant")).and_then(|t| t.as_str()).map(|s| s.to_string()),
                    _ => None,
                };
                match tenant_log {
                    Some(t) => info!(method=%method, request_id=%id_str, tenant=%t, "MCP stdio request"),
                    None => info!(method=%method, request_id=%id_str, "MCP stdio request"),
                }

                let missing = match method {
                    crate::mcp::METHOD_TOOLS_LIST => {
                        v.get("params").and_then(|p| p.get("tenant")).and_then(|t| t.as_str()).is_none()
                    }
                    crate::mcp::METHOD_TOOLS_CALL => {
                        v.get("params").and_then(|p| p.get("arguments")).and_then(|a| a.get("tenant")).and_then(|t| t.as_str()).is_none()
                    }
                    _ => false,
                };

                if missing && require_tenant {
                    let error_response = error_response(
                        Some(RequestId::new_uuid()),
                        crate::jsonrpc::JsonRpcError::invalid_request().with_data(serde_json::json!({
                            "code": "INVALID_INPUT",
                            "message": "Missing tenant (provide params.tenant or arguments.tenant)",
                        })),
                    );
                    let response_json = serde_json::to_string(&error_response)?;
                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                    continue;
                }

                // If not required, inject default tenant when missing
                if missing {
                    let default_tenant = std::env::var("OPENACT_DEFAULT_TENANT")
                        .unwrap_or_else(|_| "default".to_string());
                    match method {
                        crate::mcp::METHOD_TOOLS_LIST => {
                            match v.get_mut("params") {
                                Some(p) if p.is_object() => {
                                    p.as_object_mut().unwrap().insert(
                                        "tenant".to_string(),
                                        serde_json::Value::String(default_tenant.clone()),
                                    );
                                }
                                _ => {
                                    let mut obj = serde_json::Map::new();
                                    obj.insert("tenant".to_string(), serde_json::Value::String(default_tenant.clone()));
                                    v["params"] = serde_json::Value::Object(obj);
                                }
                            }
                        }
                        crate::mcp::METHOD_TOOLS_CALL => {
                            match v.get_mut("params") {
                                Some(p) if p.is_object() => {
                                    let pobj = p.as_object_mut().unwrap();
                                    match pobj.get_mut("arguments") {
                                        Some(args) if args.is_object() => {
                                            args.as_object_mut().unwrap().insert(
                                                "tenant".to_string(),
                                                serde_json::Value::String(default_tenant.clone()),
                                            );
                                        }
                                        _ => {
                                            pobj.insert(
                                                "arguments".to_string(),
                                                serde_json::json!({"tenant": default_tenant.clone()}),
                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    // Serialize patched request
                    if let Ok(bytes) = serde_json::to_vec(&v) {
                        maybe_patched = Some(bytes);
                    }
                }
            }
        }

        // Process the message
        let body_ref: &[u8] = if let Some(ref bytes) = maybe_patched { bytes.as_slice() } else { line.as_bytes() };
        match server.process_message(body_ref).await {
            Ok(Some(response)) => {
                // Send response
                let response_json = serde_json::to_string(&response)?;
                writeln!(stdout, "{}", response_json)?;
                stdout.flush()?;
            }
            Ok(None) => {
                // Notification - no response needed
            }
            Err(e) => {
                error!("Error processing message: {}", e);
                // Send error response
                let error_response =
                    error_response(Some(RequestId::new_uuid()), e.to_jsonrpc_error());
                let response_json = serde_json::to_string(&error_response)?;
                writeln!(stdout, "{}", response_json)?;
                stdout.flush()?;
            }
        }
    }

    info!("MCP server stopped");
    Ok(())
}

/// Serve MCP over HTTP
pub async fn serve_http(
    app_state: AppState,
    governance: GovernanceConfig,
    addr: &str,
) -> McpResult<()> {
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::Json,
        routing::post,
        Router,
    };
    use axum::extract::DefaultBodyLimit;
    use serde_json::Value;
    use uuid::Uuid;

    info!("Starting OpenAct MCP server (HTTP mode) on {}", addr);
    info!(
        "Governance: max_concurrency={}, timeout={:?}",
        governance.max_concurrency, governance.timeout
    );
    if !governance.allow_patterns.is_empty() {
        info!("Allow patterns: {:?}", governance.allow_patterns);
    }
    if !governance.deny_patterns.is_empty() {
        info!("Deny patterns: {:?}", governance.deny_patterns);
    }

    let server = Arc::new(McpServer::new(app_state, governance));

    async fn handle_mcp_request(
        State(server): State<Arc<McpServer>>,
        headers: HeaderMap,
        body: axum::body::Bytes,
    ) -> Result<(HeaderMap, Json<Value>), (StatusCode, Json<Value>)> {
        // Require JSON content type
        if let Some(ct) = headers.get("content-type") {
            let ct_str = ct.to_str().unwrap_or("").to_ascii_lowercase();
            if !ct_str.starts_with("application/json") {
                return Err((
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    Json(serde_json::json!({
                        "error": "Unsupported Media Type",
                        "expected": "application/json"
                    })),
                ));
            }
        } else {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Json(serde_json::json!({
                    "error": "Missing Content-Type",
                    "expected": "application/json"
                })),
            ));
        }
        // Validate MCP protocol version
        if let Some(protocol_version) = headers.get("mcp-protocol-version") {
            let version_str = protocol_version.to_str().unwrap_or("");
            if !crate::mcp::SUPPORTED_PROTOCOL_VERSIONS.contains(&version_str) {
                warn!("Unsupported MCP protocol version: {}", version_str);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Unsupported MCP protocol version",
                        "supported_versions": crate::mcp::SUPPORTED_PROTOCOL_VERSIONS
                    })),
                ));
            }
        }

        // Inject tenant from header if present and enforce requirement if configured
        let header_tenant = headers
            .get("x-tenant")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let require_tenant = std::env::var("OPENACT_REQUIRE_TENANT")
            .map(|v| {
                let v = v.to_ascii_lowercase();
                v == "1" || v == "true" || v == "yes"
            })
            .unwrap_or(false);

        // Try to parse the JSON-RPC request so we can inject tenant where appropriate
        let mut patched_body: Option<Vec<u8>> = None;
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&body[..]) {
            let method_opt = v.get("method").and_then(|m| m.as_str().map(|s| s.to_string()));
            if let Some(method) = method_opt {
                // Log method, jsonrpc id and tenant context (from params or header)
                let tenant_from_params = match method.as_str() {
                    crate::mcp::METHOD_TOOLS_LIST => v
                        .get("params")
                        .and_then(|p| p.get("tenant"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string()),
                    crate::mcp::METHOD_TOOLS_CALL => v
                        .get("params")
                        .and_then(|p| p.get("arguments"))
                        .and_then(|a| a.get("tenant"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string()),
                    _ => None,
                };
                let tenant_log = tenant_from_params.or(header_tenant.clone());
                let id_str = v
                    .get("id")
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "null".to_string());
                match tenant_log {
                    Some(t) => info!(method=%method, request_id=%id_str, tenant=%t, "MCP HTTP request"),
                    None => info!(method=%method, request_id=%id_str, "MCP HTTP request"),
                }
                match method.as_str() {
                    // For tools/list: place `tenant` at top-level params
                    crate::mcp::METHOD_TOOLS_LIST => {
                        let mut has_tenant = v
                            .get("params")
                            .and_then(|p| p.get("tenant"))
                            .and_then(|t| t.as_str())
                            .is_some();
                        if !has_tenant {
                            if let Some(t) = &header_tenant {
                                // create or insert tenant into params
                                match v.get_mut("params") {
                                    Some(p) if p.is_object() => {
                                        p.as_object_mut().unwrap().insert(
                                            "tenant".to_string(),
                                            serde_json::Value::String(t.clone()),
                                        );
                                    }
                                    _ => {
                                        let mut obj = serde_json::Map::new();
                                        obj.insert("tenant".to_string(), serde_json::Value::String(t.clone()));
                                        v["params"] = serde_json::Value::Object(obj);
                                    }
                                }
                                has_tenant = true;
                            }
                        }
                        if require_tenant && !has_tenant {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                Json(serde_json::json!({
                                    "error": {
                                        "code": "INVALID_INPUT",
                                        "message": "Missing tenant (provide X-Tenant header or params.tenant)",
                                    }
                                })),
                            ));
                        }
                    }
                    // For tools/call: place `tenant` under params.arguments
                    crate::mcp::METHOD_TOOLS_CALL => {
                        // Determine if a tenant exists already
                        let mut has_tenant = v
                            .get("params")
                            .and_then(|p| p.get("arguments"))
                            .and_then(|a| a.get("tenant"))
                            .and_then(|t| t.as_str())
                            .is_some();
                        if !has_tenant {
                            if let Some(t) = &header_tenant {
                                // ensure arguments is an object then insert tenant
                                match v.get_mut("params") {
                                    Some(p) if p.is_object() => {
                                        let pobj = p.as_object_mut().unwrap();
                                        match pobj.get_mut("arguments") {
                                            Some(args) if args.is_object() => {
                                                args.as_object_mut().unwrap().insert(
                                                    "tenant".to_string(),
                                                    serde_json::Value::String(t.clone()),
                                                );
                                            }
                                            Some(_) => {
                                                pobj.insert(
                                                    "arguments".to_string(),
                                                    serde_json::json!({"tenant": t.clone()}),
                                                );
                                            }
                                            None => {
                                                pobj.insert(
                                                    "arguments".to_string(),
                                                    serde_json::json!({"tenant": t.clone()}),
                                                );
                                            }
                                        }
                                    }
                                    _ => {
                                        // If params is missing or not object, avoid restructuring; rely on client or require flag
                                    }
                                }
                                has_tenant = true;
                            }
                        }
                        if require_tenant && !has_tenant {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                Json(serde_json::json!({
                                    "error": {
                                        "code": "INVALID_INPUT",
                                        "message": "Missing tenant (provide X-Tenant header or arguments.tenant)",
                                    }
                                })),
                            ));
                        }
                    }
                    _ => {}
                }

                // If we modified v, serialize back
                if v != serde_json::from_slice::<serde_json::Value>(&body[..]).unwrap_or(serde_json::Value::Null) {
                    if let Ok(bytes) = serde_json::to_vec(&v) {
                        patched_body = Some(bytes);
                    }
                }
            }
        }

        let body_ref: &[u8] = if let Some(ref b) = patched_body { b.as_slice() } else { &body[..] };

        // Process the MCP message
        match server.process_message(body_ref).await {
            Ok(Some(response)) => {
                let mut response_headers = HeaderMap::new();
                response_headers.insert("content-type", "application/json".parse().unwrap());
                response_headers.insert(
                    "mcp-protocol-version",
                    crate::mcp::LATEST_PROTOCOL_VERSION.parse().unwrap(),
                );
                response_headers
                    .insert("mcp-session-id", Uuid::new_v4().to_string().parse().unwrap());

                Ok((response_headers, Json(serde_json::to_value(response).unwrap())))
            }
            Ok(None) => {
                // Notification - no response
                let mut response_headers = HeaderMap::new();
                response_headers.insert("content-type", "application/json".parse().unwrap());
                response_headers.insert(
                    "mcp-protocol-version",
                    crate::mcp::LATEST_PROTOCOL_VERSION.parse().unwrap(),
                );
                response_headers
                    .insert("mcp-session-id", Uuid::new_v4().to_string().parse().unwrap());

                Ok((response_headers, Json(serde_json::json!({}))))
            }
            Err(e) => {
                error!("Error processing MCP request: {}", e);

                // Map MCP errors to HTTP status codes
                let status = match e {
                    McpError::InvalidArguments(_) => StatusCode::BAD_REQUEST,
                    McpError::PermissionDenied(_) => StatusCode::FORBIDDEN,
                    McpError::ToolNotFound(_) => StatusCode::NOT_FOUND,
                    McpError::Timeout => StatusCode::REQUEST_TIMEOUT,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };

                let error_response = e.to_jsonrpc_error();
                Err((status, Json(serde_json::to_value(error_response).unwrap())))
            }
        }
    }

    // Limit request body size for safety (1 MiB default here)
    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
        .layer(DefaultBodyLimit::max(1 * 1024 * 1024))
        .with_state(server);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| McpError::Internal(format!("Failed to bind to {}: {}", addr, e)))?;

    info!("HTTP MCP server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| McpError::Internal(format!("HTTP server error: {}", e)))?;

    Ok(())
}
