use crate::{adapter::McpAdapter, AppState, GovernanceConfig, McpResult};
use openact_protocol_adapter::{dto::InvokeRequest, mcp as adapter_mcp};
use openact_protocol_adapter::traits::{ToolCatalog, ToolInvoker};
use rmcp::{
    model as m,
    service::{serve_server, RoleServer},
    handler::server::ServerHandler,
    service::RequestContext,
};

pub struct RmcpOpenActServer {
    adapter: McpAdapter,
}

impl RmcpOpenActServer {
    pub fn new(adapter: McpAdapter) -> Self { Self { adapter } }
}

impl ServerHandler for RmcpOpenActServer {
    fn get_info(&self) -> m::ServerInfo { m::ServerInfo::default() }

    fn list_tools(
        &self,
        _request: Option<m::PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let specs = self
                .adapter
                .list_tools(None)
                .await
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), e.data))?;
            let tools: Vec<m::Tool> = specs.iter().map(adapter_mcp::to_mcp_tool).collect();
            Ok(m::ListToolsResult { tools, next_cursor: None })
        }
    }

    fn call_tool(
        &self,
        request: m::CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let args = request
                .arguments
                .as_ref()
                .map(|m| serde_json::Value::Object(m.clone()))
                .unwrap_or_else(|| serde_json::json!({}));
            let tenant = args.get("tenant").and_then(|v| v.as_str()).map(|s| s.to_string());
            let req = InvokeRequest { tool: request.name.to_string(), tenant, args };
            let res = self
                .adapter
                .invoke(req)
                .await
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), e.data))?;
            Ok(adapter_mcp::to_mcp_call_result(&res))
        }
    }
}

/// Serve MCP using official rmcp stdio transport
pub async fn serve_stdio_rmcp(app_state: AppState, governance: GovernanceConfig) -> McpResult<()> {
    let adapter = McpAdapter::new(app_state, governance);
    let server = RmcpOpenActServer::new(adapter);
    let running = serve_server(server, (tokio::io::stdin(), tokio::io::stdout()))
        .await
        .map_err(|e| crate::error::McpError::Internal(format!("rmcp init error: {}", e)))?;
    // block until quit
    let _ = running.waiting().await;
    Ok(())
}
