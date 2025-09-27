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
        Content, Implementation, InitializeRequest, InitializeResponse, ServerCapabilities, Tool,
        ToolsCallRequest, ToolsCallResponse, ToolsCapability, ToolsListRequest, ToolsListResponse,
        LATEST_PROTOCOL_VERSION, METHOD_INITIALIZE, METHOD_PING, METHOD_TOOLS_CALL,
        METHOD_TOOLS_LIST, SUPPORTED_PROTOCOL_VERSIONS,
    },
    AppState, GovernanceConfig, McpError, McpResult,
};
use openact_core::store::{ActionRepository, ConnectionStore};
use openact_core::{ConnectorKind, Trn};
#[allow(unused_imports)]
use openact_registry::HttpFactory;
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
        // Register HTTP factories (others can be added later)
        registry.register_connection_factory(Arc::new(HttpFactory::new()));
        registry.register_action_factory(Arc::new(HttpFactory::new()));

        Self {
            app_state,
            registry,
            governance,
        }
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
                tools: Some(ToolsCapability { list_changed: None }),
                resources: None,
                prompts: None,
            },
            server_info: Implementation {
                name: "OpenAct MCP Server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "OpenAct MCP Server - Execute actions through connectors".to_string(),
            ),
        };

        Ok(success_response(
            request.id.clone(),
            serde_json::to_value(response)?,
        ))
    }

    /// Handle ping method
    async fn handle_ping(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        Ok(success_response(request.id.clone(), serde_json::json!({})))
    }

    /// Handle tools/list method
    async fn handle_tools_list(&self, request: &JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let _params: ToolsListRequest = if let Some(params) = &request.params {
            serde_json::from_value(params.clone())?
        } else {
            ToolsListRequest { cursor: None }
        };
        // Dynamic tools from store (mcp_enabled)
        let mut tools: Vec<Tool> = Vec::new();

        // Include generic executor if allowed by governance
        let openact_execute_name = "openact.execute";
        if self.governance.is_tool_allowed(openact_execute_name) {
            tools.push(Tool {
                name: openact_execute_name.to_string(),
                description: Some("Execute an OpenAct action using either explicit TRN or connector/action components".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action_trn": {
                            "type": "string",
                            "description": "Explicit action TRN (e.g., 'trn:openact:tenant:action/http/get@v1')"
                        },
                        "connector": {
                            "type": "string",
                            "description": "Connector type (e.g., 'http') - required when action_trn not provided"
                        },
                        "action": {
                            "type": "string",
                            "description": "Action name (e.g., 'get') - required when action_trn not provided"
                        },
                        "tenant": {
                            "type": "string",
                            "description": "Tenant name (default: 'default')"
                        },
                        "version": {
                            "type": "integer",
                            "description": "Action version (default: latest)"
                        },
                        "input": {
                            "type": "object",
                            "description": "Input parameters for the action"
                        }
                    },
                    "required": ["input"],
                    "oneOf": [
                        {
                            "required": ["action_trn", "input"]
                        },
                        {
                            "required": ["connector", "action", "input"]
                        }
                    ]
                }),
            });
        } else {
            debug!(
                "Tool '{}' filtered by governance policy",
                openact_execute_name
            );
        }

        // Optimize: Get all MCP-enabled actions in one query to avoid N+1
        let all_actions = self.get_all_mcp_enabled_actions().await?;
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

            // Determine description: use mcp_overrides.description if available
            let description = a.mcp_overrides.as_ref().and_then(|o| o.description.clone());

            // TODO: Generate schema from ActionInput definition in the future
            // For now, use a permissive schema that accepts any object input
            let schema = serde_json::json!({
                "type": "object",
                "additionalProperties": true,
                "description": "Action-specific input parameters"
            });

            tools.push(Tool {
                name: tool_name,
                description,
                input_schema: schema,
            });
        }

        // Log alias conflicts if any
        if !alias_conflicts.is_empty() {
            warn!(
                "Detected {} tool name conflicts: {:?}",
                alias_conflicts.len(),
                alias_conflicts
            );
        }

        let response = ToolsListResponse {
            tools,
            next_cursor: None,
        };

        Ok(success_response(
            request.id.clone(),
            serde_json::to_value(response)?,
        ))
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
        let _permit = self
            .governance
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| McpError::Internal("Failed to acquire concurrency permit".to_string()))?;

        debug!(
            "Acquired concurrency permit for tool: {}",
            call_request.name
        );

        // Execute with timeout
        let execution_future = async {
            match call_request.name.as_str() {
                "openact.execute" => {
                    let result = self.execute_openact_action(&call_request.arguments).await?;
                    Ok(success_response(
                        request.id.clone(),
                        serde_json::to_value(result)?,
                    ))
                }
                // For per-action tools (both direct connector.action and aliased tools)
                other => {
                    let (connector, action) = self.resolve_tool_to_action(other).await?;

                    // The tool's arguments are treated as the action input directly
                    let wrapped = serde_json::json!({
                        "connector": connector,
                        "action": action,
                        "input": call_request.arguments
                    });
                    let result = self.execute_openact_action(&wrapped).await?;
                    Ok(success_response(
                        request.id.clone(),
                        serde_json::to_value(result)?,
                    ))
                }
            }
        };

        // Apply timeout
        match timeout(self.governance.timeout, execution_future).await {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "Tool '{}' timed out after {:?}",
                    call_request.name, self.governance.timeout
                );
                Err(McpError::Timeout)
            }
        }
    }

    /// Get all MCP-enabled actions in a single query to avoid N+1 problem
    async fn get_all_mcp_enabled_actions(
        &self,
    ) -> McpResult<Vec<openact_core::types::ActionRecord>> {
        let connectors = ConnectionStore::list_distinct_connectors(self.app_state.store.as_ref())
            .await
            .map_err(|e| McpError::Internal(format!("Failed to list connectors: {}", e)))?;

        let mut all_actions = Vec::new();
        for kind in connectors {
            let actions = ActionRepository::list_by_connector(self.app_state.store.as_ref(), &kind)
                .await
                .map_err(|e| {
                    McpError::Internal(format!(
                        "Failed to list actions for {}: {}",
                        kind.as_str(),
                        e
                    ))
                })?;

            for action in actions {
                if action.mcp_enabled {
                    all_actions.push(action);
                }
            }
        }

        Ok(all_actions)
    }

    /// Resolve tool name to (connector, action) pair
    async fn resolve_tool_to_action(&self, tool_name: &str) -> McpResult<(String, String)> {
        // First try to find it as an alias in mcp_overrides.tool_name
        let connectors = ConnectionStore::list_distinct_connectors(self.app_state.store.as_ref())
            .await
            .map_err(|e| McpError::Internal(format!("Failed to list connectors: {}", e)))?;

        for kind in connectors {
            let actions = ActionRepository::list_by_connector(self.app_state.store.as_ref(), &kind)
                .await
                .map_err(|e| McpError::Internal(format!("Failed to list actions: {}", e)))?;

            for a in actions {
                if !a.mcp_enabled {
                    continue;
                }

                // Check if this action has the tool name as an alias
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
        }

        // If not found as alias, try direct connector.action format
        if tool_name.contains('.') {
            let mut parts = tool_name.splitn(2, '.');
            let connector = parts.next().unwrap_or("");
            let action = parts.next().unwrap_or("");
            if !connector.is_empty() && !action.is_empty() {
                // Verify this action exists
                let kind = ConnectorKind::new(connector.to_string());
                let actions =
                    ActionRepository::list_by_connector(self.app_state.store.as_ref(), &kind)
                        .await
                        .map_err(|e| {
                            McpError::Internal(format!("Failed to list actions: {}", e))
                        })?;

                if actions.iter().any(|a| a.name == action && a.mcp_enabled) {
                    debug!(
                        "Resolved direct tool '{}' to {}.{}",
                        tool_name, connector, action
                    );
                    return Ok((connector.to_string(), action.to_string()));
                }
            }
        }

        Err(McpError::ToolNotFound(format!(
            "Tool not found: {}",
            tool_name
        )))
    }

    /// Execute an OpenAct action
    async fn execute_openact_action(&self, arguments: &Value) -> McpResult<ToolsCallResponse> {
        // Validate arguments object
        if !arguments.is_object() {
            return Err(McpError::InvalidArguments(
                "Arguments must be an object".to_string(),
            ));
        }

        let input = arguments
            .get("input")
            .ok_or_else(|| McpError::InvalidArguments("Missing 'input' field".to_string()))?;

        // Check if explicit action_trn is provided
        let action_trn = if let Some(trn_str) = arguments.get("action_trn").and_then(|v| v.as_str())
        {
            // Use explicit TRN - validate it exists in the database
            let trn = Trn::new(trn_str.to_string());
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

            let tenant = arguments
                .get("tenant")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let version_opt = arguments.get("version").and_then(|v| v.as_i64());

            info!(
                "Resolving action: {}.{} (tenant={} version={:?})",
                connector, action, tenant, version_opt
            );

            // Resolve action TRN by scanning actions of the connector
            let kind = ConnectorKind::new(connector.to_string());
            let actions = ActionRepository::list_by_connector(self.app_state.store.as_ref(), &kind)
                .await
                .map_err(|e| McpError::Internal(format!("Failed to list actions: {}", e)))?;

            let mut candidates: Vec<_> = actions
                .into_iter()
                .filter(|a| {
                    debug!("Checking action: name='{}' vs target='{}'", a.name, action);
                    a.name == action
                })
                .filter(|a| {
                    if let Some(parsed) = a.trn.parse_action() {
                        debug!(
                            "TRN '{}' parsed: tenant='{}' vs target='{}'",
                            a.trn.as_str(),
                            parsed.tenant,
                            tenant
                        );
                        parsed.tenant == tenant
                    } else {
                        debug!("Failed to parse TRN: {}", a.trn.as_str());
                        false
                    }
                })
                .collect();

            if candidates.is_empty() {
                return Err(McpError::ToolNotFound(format!(
                    "Action not found: {}.{} (tenant: {})",
                    connector, action, tenant
                )));
            }

            // Sort by version and pick the appropriate one
            candidates.sort_by_key(|a| a.trn.parse_action().map(|c| c.version).unwrap_or(0));
            let chosen = if let Some(v) = version_opt {
                candidates
                    .into_iter()
                    .rev()
                    .find(|a| {
                        a.trn
                            .parse_action()
                            .map(|c| c.version == v)
                            .unwrap_or(false)
                    })
                    .ok_or_else(|| {
                        McpError::ToolNotFound(format!(
                            "Action not found: {}.{}@v{} (tenant: {})",
                            connector, action, v, tenant
                        ))
                    })?
            } else {
                candidates.pop().unwrap()
            };

            chosen.trn
        };
        let ctx = ExecutionContext::new();
        let exec = self
            .registry
            .execute(&action_trn, input.clone(), Some(ctx))
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;

        let text = serde_json::to_string(&exec.output).unwrap_or_else(|_| "{}".to_string());
        Ok(ToolsCallResponse {
            content: vec![Content::text(text)],
            is_error: None,
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

    let server = McpServer::new(app_state, governance);
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

        // Process the message
        match server.process_message(line.as_bytes()).await {
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

        // Process the MCP message
        match server.process_message(&body[..]).await {
            Ok(Some(response)) => {
                let mut response_headers = HeaderMap::new();
                response_headers.insert("content-type", "application/json".parse().unwrap());
                response_headers.insert(
                    "mcp-protocol-version",
                    crate::mcp::LATEST_PROTOCOL_VERSION.parse().unwrap(),
                );
                response_headers.insert(
                    "mcp-session-id",
                    Uuid::new_v4().to_string().parse().unwrap(),
                );

                Ok((
                    response_headers,
                    Json(serde_json::to_value(response).unwrap()),
                ))
            }
            Ok(None) => {
                // Notification - no response
                let mut response_headers = HeaderMap::new();
                response_headers.insert("content-type", "application/json".parse().unwrap());
                response_headers.insert(
                    "mcp-protocol-version",
                    crate::mcp::LATEST_PROTOCOL_VERSION.parse().unwrap(),
                );
                response_headers.insert(
                    "mcp-session-id",
                    Uuid::new_v4().to_string().parse().unwrap(),
                );

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

    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
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
