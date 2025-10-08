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

    async fn load_actions(
        &self,
        tenant: Option<&str>,
        connector: Option<ConnectorKind>,
    ) -> Result<Vec<ActionRecord>, ProtocolError> {
        let mut filter = ActionListFilter { mcp_enabled: Some(true), ..Default::default() };
        if let Some(t) = tenant {
            filter.tenant = Some(t.to_string());
        }
        if let Some(kind) = connector {
            filter.connector = Some(kind);
        }
        filter.allow_patterns = Some(self.governance.allow_patterns.clone());
        filter.deny_patterns = Some(self.governance.deny_patterns.clone());
        ActionRepository::list_filtered(self.app_state.store.as_ref(), filter, None)
            .await
            .map_err(|e| ProtocolError::new("STORE_ERROR", format!("list_filtered: {}", e), None))
    }

    fn resolve_alias_or_direct(
        &self,
        tool_name: &str,
        actions: &[ActionRecord],
    ) -> Result<(String, String), ProtocolError> {
        for record in actions {
            if let Some(ref overrides) = record.mcp_overrides {
                if let Some(ref alias) = overrides.tool_name {
                    if alias == tool_name {
                        return Ok((record.connector.as_str().to_string(), record.name.clone()));
                    }
                }
            }
        }
        if let Some(parsed) = openact_core::types::ToolName::parse_human(tool_name) {
            return Ok((parsed.connector.to_string(), parsed.action.to_string()));
        }
        Err(ProtocolError::new("TOOL_NOT_FOUND", format!("tool {} not found", tool_name), None))
    }

    fn select_action_record(
        &self,
        actions: &[ActionRecord],
        connector: &ConnectorKind,
        action: &str,
        version: Option<i64>,
        tenant: Option<&str>,
    ) -> Result<ActionRecord, ProtocolError> {
        let target_connector = connector.as_str().to_string();
        let mut candidates: Vec<&ActionRecord> = actions
            .iter()
            .filter(|record| {
                record.connector.canonical().0 == target_connector && record.name == action
            })
            .collect();
        if candidates.is_empty() {
            return Err(ProtocolError::new(
                "TOOL_NOT_FOUND",
                format!("action {}.{} not found", connector.as_str(), action),
                None,
            ));
        }
        if let Some(v) = version {
            candidates.retain(|record| record.version == v);
            if candidates.is_empty() {
                return Err(ProtocolError::new(
                    "VERSION_NOT_FOUND",
                    format!("version {} not found for {}.{}", v, connector.as_str(), action),
                    None,
                ));
            }
        } else {
            let max_version = candidates.iter().map(|record| record.version).max().unwrap_or(0);
            candidates.retain(|record| record.version == max_version);
        }
        let picked = candidates[0];
        self.ensure_action_access(picked, tenant)?;
        Ok(picked.clone())
    }

    async fn action_by_trn(
        &self,
        trn: &Trn,
        tenant: Option<&str>,
    ) -> Result<ActionRecord, ProtocolError> {
        let record = ActionRepository::get(self.app_state.store.as_ref(), trn)
            .await
            .map_err(|e| ProtocolError::new("STORE_ERROR", format!("get: {}", e), None))?
            .ok_or_else(|| {
                ProtocolError::new("TOOL_NOT_FOUND", format!("action {} not found", trn), None)
            })?;
        self.ensure_action_access(&record, tenant)?;
        Ok(record)
    }

    fn ensure_action_access(
        &self,
        record: &ActionRecord,
        tenant: Option<&str>,
    ) -> Result<(), ProtocolError> {
        if !record.mcp_enabled {
            return Err(ProtocolError::new(
                "FORBIDDEN",
                format!("action {} is not enabled for MCP", record.trn),
                None,
            ));
        }
        if let Some(request_tenant) = tenant {
            if let Some(components) = record.trn.parse_action() {
                if components.tenant != request_tenant {
                    return Err(ProtocolError::new(
                        "FORBIDDEN",
                        format!("action {} not visible for tenant {}", record.trn, request_tenant),
                        None,
                    ));
                }
            }
        }
        let tool_name = Self::tool_name_for_record(record);
        if !self.governance.is_tool_allowed(&tool_name) {
            return Err(ProtocolError::new(
                "FORBIDDEN",
                format!("tool {} is not allowed by governance", tool_name),
                None,
            ));
        }
        Ok(())
    }

    fn tool_name_for_record(record: &ActionRecord) -> String {
        record
            .mcp_overrides
            .as_ref()
            .and_then(|overrides| overrides.tool_name.clone())
            .unwrap_or_else(|| format!("{}.{}", record.connector.as_str(), record.name))
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
            let actions = self.load_actions(tenant, None).await?;
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

            if req.tool == "openact.execute" {
                if let Some(trn_str) = args.get("action_trn").and_then(|v| v.as_str()) {
                    let trn = Trn::new(trn_str.to_string());
                    let record = self.action_by_trn(&trn, tenant_opt).await?;
                    let input = args.get("input").cloned().unwrap_or_else(|| json!({}));
                    let exec =
                        self.registry.execute(&record.trn, input, None).await.map_err(|e| {
                            ProtocolError::new("EXEC_ERROR", format!("{}", e), None)
                        })?;
                    return Ok(InvokeResult { structured: exec.output, text_fallback: None });
                }

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
                let connector_kind = ConnectorKind::new(connector).canonical();
                let actions = self.load_actions(tenant_opt, Some(connector_kind.clone())).await?;
                let record = self.select_action_record(
                    &actions,
                    &connector_kind,
                    action,
                    version_opt,
                    tenant_opt,
                )?;
                let exec = self
                    .registry
                    .execute(&record.trn, input, None)
                    .await
                    .map_err(|e| ProtocolError::new("EXEC_ERROR", format!("{}", e), None))?;
                return Ok(InvokeResult { structured: exec.output, text_fallback: None });
            }

            let actions = self.load_actions(tenant_opt, None).await?;
            let (connector_name, action_name) =
                self.resolve_alias_or_direct(&req.tool, &actions)?;
            let version_opt = args.get("version").and_then(|v| v.as_i64());
            let input = args.get("input").cloned().unwrap_or_else(|| json!({}));
            let connector_kind = ConnectorKind::new(&connector_name).canonical();
            let record = self.select_action_record(
                &actions,
                &connector_kind,
                &action_name,
                version_opt,
                tenant_opt,
            )?;
            let exec = self
                .registry
                .execute(&record.trn, input, None)
                .await
                .map_err(|e| ProtocolError::new("EXEC_ERROR", format!("{}", e), None))?;
            Ok(InvokeResult { structured: exec.output, text_fallback: None })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use openact_core::store::{ActionRepository, ConnectionStore};
    use openact_core::types::ConnectorMetadata;
    use openact_core::types::McpOverrides;
    use openact_core::{ActionRecord, ConnectionRecord};
    use openact_registry::factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory};
    use openact_store::SqlStore;
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct TestFactory {
        executions: Arc<AtomicUsize>,
    }

    struct TestConnection {
        trn: Trn,
        connector: ConnectorKind,
    }

    struct TestAction {
        trn: Trn,
        connector: ConnectorKind,
        executions: Arc<AtomicUsize>,
    }

    impl AsAny for TestConnection {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[async_trait::async_trait]
    impl Connection for TestConnection {
        fn trn(&self) -> &Trn {
            &self.trn
        }

        fn connector_kind(&self) -> &ConnectorKind {
            &self.connector
        }

        async fn health_check(&self) -> openact_registry::RegistryResult<bool> {
            Ok(true)
        }

        fn metadata(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }
    }

    #[async_trait::async_trait]
    impl ConnectionFactory for TestFactory {
        fn connector_kind(&self) -> ConnectorKind {
            ConnectorKind::new("test")
        }

        fn metadata(&self) -> ConnectorMetadata {
            ConnectorMetadata {
                kind: ConnectorKind::new("test"),
                display_name: "Test Connector".into(),
                description: "Test connector for MCP adapter".into(),
                category: "test".into(),
                supported_operations: vec![],
                supports_auth: false,
                example_config: None,
                version: "1.0".into(),
            }
        }

        async fn create_connection(
            &self,
            record: &ConnectionRecord,
        ) -> openact_registry::RegistryResult<Arc<dyn Connection>> {
            Ok(Arc::new(TestConnection {
                trn: record.trn.clone(),
                connector: record.connector.clone(),
            }))
        }
    }

    impl AsAny for TestAction {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[async_trait::async_trait]
    impl Action for TestAction {
        fn trn(&self) -> &Trn {
            &self.trn
        }

        fn connector_kind(&self) -> &ConnectorKind {
            &self.connector
        }

        async fn execute(
            &self,
            _input: serde_json::Value,
        ) -> openact_registry::RegistryResult<serde_json::Value> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            Ok(json!({ "echo": "ok" }))
        }

        fn metadata(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }
    }

    #[async_trait::async_trait]
    impl ActionFactory for TestFactory {
        fn connector_kind(&self) -> ConnectorKind {
            ConnectorKind::new("test")
        }

        fn metadata(&self) -> ConnectorMetadata {
            <Self as ConnectionFactory>::metadata(self)
        }

        async fn create_action(
            &self,
            action_record: &ActionRecord,
            _connection: Arc<dyn Connection>,
        ) -> openact_registry::RegistryResult<Box<dyn Action>> {
            Ok(Box::new(TestAction {
                trn: action_record.trn.clone(),
                connector: action_record.connector.clone(),
                executions: self.executions.clone(),
            }))
        }
    }

    #[tokio::test]
    async fn openact_execute_trn_enforces_mcp_and_governance() {
        let store = Arc::new(SqlStore::new("sqlite::memory:?cache=shared").await.unwrap());
        let app_state = AppState::from_arc(store.clone());

        let executions = Arc::new(AtomicUsize::new(0));
        let factory = Arc::new(TestFactory { executions: executions.clone() });

        let conn_store = store.as_ref().clone();
        let act_repo = store.as_ref().clone();
        let mut registry = ConnectorRegistry::new(conn_store, act_repo);
        registry.register_connection_factory(factory.clone());
        registry.register_action_factory(factory);

        let tenant = "tenant";
        let connector = ConnectorKind::new("test");
        let connection_trn = Trn::new(format!("trn:openact:{}:connection/test/conn@v1", tenant));
        let action_trn = Trn::new(format!("trn:openact:{}:action/test/do@v1", tenant));
        let now = Utc::now();

        ConnectionStore::upsert(
            store.as_ref(),
            &ConnectionRecord {
                trn: connection_trn.clone(),
                connector: connector.clone(),
                name: "conn".into(),
                config_json: json!({}),
                created_at: now,
                updated_at: now,
                version: 1,
            },
        )
        .await
        .unwrap();

        let mut action_record = ActionRecord {
            trn: action_trn.clone(),
            connector: connector.clone(),
            name: "do".into(),
            connection_trn: connection_trn.clone(),
            config_json: json!({}),
            mcp_enabled: false,
            mcp_overrides: None,
            created_at: now,
            updated_at: now,
            version: 1,
        };
        ActionRepository::upsert(store.as_ref(), &action_record).await.unwrap();

        let mut adapter = McpAdapter {
            app_state,
            registry,
            governance: GovernanceConfig::new(vec![], vec![], 4, 30),
        };

        let request = InvokeRequest {
            tool: "openact.execute".to_string(),
            tenant: Some(tenant.to_string()),
            args: json!({ "action_trn": action_trn.as_str() }),
        };
        let err = adapter.invoke(request).await.expect_err("mcp disabled should block");
        assert_eq!(err.code, "FORBIDDEN");
        assert!(err.message.contains("not enabled"), "message={}", err.message);

        action_record.mcp_enabled = true;
        action_record.mcp_overrides = Some(McpOverrides {
            tool_name: Some("denied.tool".into()),
            description: None,
            tags: vec![],
            requires_auth: false,
        });
        ActionRepository::upsert(store.as_ref(), &action_record).await.unwrap();
        adapter.governance = GovernanceConfig::new(vec![], vec!["denied.*".into()], 4, 30);

        let request = InvokeRequest {
            tool: "openact.execute".to_string(),
            tenant: Some(tenant.to_string()),
            args: json!({ "action_trn": action_trn.as_str() }),
        };
        let err = adapter.invoke(request).await.expect_err("governance deny should block");
        assert_eq!(err.code, "FORBIDDEN");
        assert!(err.message.contains("not allowed"), "message={}", err.message);

        action_record.mcp_overrides = Some(McpOverrides {
            tool_name: Some("allowed.tool".into()),
            description: None,
            tags: vec![],
            requires_auth: false,
        });
        ActionRepository::upsert(store.as_ref(), &action_record).await.unwrap();
        adapter.governance = GovernanceConfig::new(
            vec!["openact.execute".into(), "allowed.*".into()],
            vec![],
            4,
            30,
        );

        let request = InvokeRequest {
            tool: "openact.execute".to_string(),
            tenant: Some(tenant.to_string()),
            args: json!({ "action_trn": action_trn.as_str(), "input": { "echo": "ok" } }),
        };
        let result = adapter.invoke(request).await.expect("governance should allow execution");
        assert_eq!(result.structured, json!({ "echo": "ok" }));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
    }
}
