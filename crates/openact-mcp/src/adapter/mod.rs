use openact_core::store::{ActionListFilter, ActionRepository};
use openact_core::types::Trn;
use openact_core::{ActionRecord, ConnectorKind};
use openact_registry::ConnectorRegistry;
use serde_json::json;
use std::collections::HashSet;

use crate::{AppState, GovernanceConfig};
use openact_protocol_adapter::dto::{InvokeRequest, InvokeResult, ProtocolError, ToolSpec};
use openact_protocol_adapter::traits::{ToolCatalog, ToolInvoker};

/// Protocol adapter that bridges rmcp <-> OpenAct registry/store/governance.
///
/// Note: initial scaffold. Implementations will be filled in subsequent steps.
pub struct McpAdapter {
    pub app_state: AppState,
    pub registry: ConnectorRegistry,
    pub governance: GovernanceConfig,
}

impl McpAdapter {
    pub fn new(app_state: AppState, governance: GovernanceConfig) -> Self {
        // Build registry from shared SqlStore (same as server::McpServer::new)
        let store_arc = app_state.store.clone();
        let conn_store = store_arc.as_ref().clone();
        let act_repo = store_arc.as_ref().clone();
        let mut registry = ConnectorRegistry::new(conn_store, act_repo);
        for registrar in openact_plugins::registrars() {
            registrar(&mut registry);
        }
        Self { app_state, registry, governance }
    }

    async fn resolve_alias_or_direct(
        &self,
        tool_name: &str,
        tenant: Option<&str>,
    ) -> Result<(String, String), ProtocolError> {
        // 1) Check alias via action.mcp_overrides.tool_name
        let mut filter = ActionListFilter { mcp_enabled: Some(true), ..Default::default() };
        if let Some(t) = tenant {
            filter.tenant = Some(t.to_string());
        }
        // Apply governance patterns so we don't leak disallowed tools
        filter.allow_patterns = Some(self.governance.allow_patterns.clone());
        filter.deny_patterns = Some(self.governance.deny_patterns.clone());
        let actions = ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
            .await
            .map_err(|e| {
                ProtocolError::new("STORE_ERROR", format!("list_filtered: {}", e), None)
            })?;
        for a in &actions {
            if let Some(ref ov) = a.mcp_overrides {
                if let Some(ref alias) = ov.tool_name {
                    if alias == tool_name {
                        return Ok((a.connector.as_str().to_string(), a.name.clone()));
                    }
                }
            }
        }
        // 2) Direct connector.action
        if let Some(parsed) = openact_core::types::ToolName::parse_human(tool_name) {
            return Ok((parsed.connector.to_string(), parsed.action.to_string()));
        }
        Err(ProtocolError::new("TOOL_NOT_FOUND", format!("tool {} not found", tool_name), None))
    }

    async fn find_action_record(
        &self,
        connector: &str,
        action: &str,
        tenant: Option<&str>,
        version: Option<i64>,
    ) -> Result<ActionRecord, ProtocolError> {
        let kind = ConnectorKind::new(connector).canonical();
        let mut filter = ActionListFilter {
            connector: Some(kind),
            mcp_enabled: Some(true),
            ..Default::default()
        };
        if let Some(t) = tenant {
            filter.tenant = Some(t.to_string());
        }
        filter.allow_patterns = Some(self.governance.allow_patterns.clone());
        filter.deny_patterns = Some(self.governance.deny_patterns.clone());
        let actions = ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
            .await
            .map_err(|e| {
                ProtocolError::new("STORE_ERROR", format!("list_filtered: {}", e), None)
            })?;
        let mut candidates: Vec<&ActionRecord> =
            actions.iter().filter(|a| a.name == action).collect();
        if candidates.is_empty() {
            return Err(ProtocolError::new(
                "TOOL_NOT_FOUND",
                format!("action {}.{} not found", connector, action),
                None,
            ));
        }
        if let Some(v) = version {
            candidates.retain(|a| a.version == v);
            if candidates.is_empty() {
                return Err(ProtocolError::new(
                    "VERSION_NOT_FOUND",
                    format!("version {} not found for {}.{}", v, connector, action),
                    None,
                ));
            }
        } else {
            // pick highest version
            let max_v = candidates.iter().map(|a| a.version).max().unwrap_or(0);
            candidates.retain(|a| a.version == max_v);
        }
        Ok((*candidates[0]).clone())
    }
}

impl ToolCatalog for McpAdapter {
    fn list_tools<'a>(
        &'a self,
        tenant: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<ToolSpec>, ProtocolError>> + Send + 'a>,
    > {
        Box::pin(async move {
            // Build filter
            let mut filter = ActionListFilter { mcp_enabled: Some(true), ..Default::default() };
            if let Some(t) = tenant {
                filter.tenant = Some(t.to_string());
            }
            filter.allow_patterns = Some(self.governance.allow_patterns.clone());
            filter.deny_patterns = Some(self.governance.deny_patterns.clone());
            let actions =
                ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
                    .await
                    .map_err(|e| {
                        ProtocolError::new("STORE_ERROR", format!("list_filtered: {}", e), None)
                    })?;

            let mut seen: HashSet<String> = HashSet::new();
            let mut specs: Vec<ToolSpec> = Vec::new();

            // Optional generic executor tool
            let openact_execute_name = "openact.execute";
            if self.governance.is_tool_allowed(openact_execute_name) {
                if seen.insert(openact_execute_name.to_string()) {
                    specs.push(ToolSpec {
                        name: openact_execute_name.to_string(),
                        title: Some("OpenAct Execute".to_string()),
                        description: Some(
                            "Execute an OpenAct action by TRN or connector/action".to_string(),
                        ),
                        annotations: None,
                        input_schema: json!({
                            "action_trn": {"type": "string"},
                            "connector": {"type": "string"},
                            "action": {"type": "string"},
                            "tenant": {"type": "string"},
                            "version": {"type": "integer"},
                            "input": {"type": "object"}
                        }),
                        output_schema: None,
                    });
                }
            }

            for a in actions {
                let tool_name = a
                    .mcp_overrides
                    .as_ref()
                    .and_then(|o| o.tool_name.clone())
                    .unwrap_or_else(|| format!("{}.{}", a.connector.as_str(), a.name));
                if !self.governance.is_tool_allowed(&tool_name) {
                    continue;
                }
                if !seen.insert(tool_name.clone()) {
                    continue;
                }
                // derive schemas/annotations
                let (input_schema, output_schema) =
                    self.registry.derive_mcp_schemas(&a).await.map_err(|e| {
                        ProtocolError::new("SCHEMA_DERIVE_ERROR", format!("{}", e), None)
                    })?;
                let annotations = self.registry.derive_mcp_annotations(&a).await.map_err(|e| {
                    ProtocolError::new("ANNOTATIONS_DERIVE_ERROR", format!("{}", e), None)
                })?;

                let description = a.mcp_overrides.as_ref().and_then(|o| o.description.clone());

                specs.push(ToolSpec {
                    name: tool_name,
                    title: None,
                    description,
                    annotations,
                    input_schema,
                    output_schema,
                });
            }
            Ok(specs)
        })
    }
}

impl ToolInvoker for McpAdapter {
    fn invoke<'a>(
        &'a self,
        req: InvokeRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<InvokeResult, ProtocolError>> + Send + 'a>,
    > {
        Box::pin(async move {
            // Governance gate
            if !self.governance.is_tool_allowed(&req.tool) {
                return Err(ProtocolError::new(
                    "FORBIDDEN",
                    format!("tool {} is not allowed", req.tool),
                    None,
                ));
            }

            let tenant_opt = req.tenant.as_deref();
            let args = req.args;

            // openact.execute supports direct TRN or connector/action
            if req.tool == "openact.execute" {
                // If action_trn present
                if let Some(trn_str) = args.get("action_trn").and_then(|v| v.as_str()) {
                    let trn = Trn::new(trn_str.to_string());
                    let input = args.get("input").cloned().unwrap_or_else(|| json!({}));
                    let exec =
                        self.registry.execute(&trn, input, None).await.map_err(|e| {
                            ProtocolError::new("EXEC_ERROR", format!("{}", e), None)
                        })?;
                    return Ok(InvokeResult { structured: exec.output, text_fallback: None });
                }
                // Else connector+action
                let connector =
                    args.get("connector").and_then(|v| v.as_str()).ok_or_else(|| {
                        ProtocolError::new("INVALID_INPUT", "missing connector", None)
                    })?;
                let action = args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ProtocolError::new("INVALID_INPUT", "missing action", None))?;
                let version_opt = args.get("version").and_then(|v| v.as_i64());
                let input = args.get("input").cloned().unwrap_or_else(|| json!({}));
                let rec =
                    self.find_action_record(connector, action, tenant_opt, version_opt).await?;
                let exec = self
                    .registry
                    .execute(&rec.trn, input, None)
                    .await
                    .map_err(|e| ProtocolError::new("EXEC_ERROR", format!("{}", e), None))?;
                return Ok(InvokeResult { structured: exec.output, text_fallback: None });
            }

            // Per-action tools: resolve alias or direct
            let (connector, action) = self.resolve_alias_or_direct(&req.tool, tenant_opt).await?;
            let version_opt = args.get("version").and_then(|v| v.as_i64());
            let input = args.get("input").cloned().unwrap_or_else(|| json!({}));
            let rec = self.find_action_record(&connector, &action, tenant_opt, version_opt).await?;
            let exec = self
                .registry
                .execute(&rec.trn, input, None)
                .await
                .map_err(|e| ProtocolError::new("EXEC_ERROR", format!("{}", e), None))?;
            Ok(InvokeResult { structured: exec.output, text_fallback: None })
        })
    }
}
