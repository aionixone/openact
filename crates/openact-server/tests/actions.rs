use async_trait::async_trait;
use axum::{
    body::{self, Body},
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use openact_core::{
    store::{ActionRepository, ConnectionStore},
    types::{ActionRecord, ConnectionRecord, ConnectorKind, Trn},
};
use openact_server::{restapi::create_router, AppState};
use openact_store::SqlStore;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::sleep;
use tower::ServiceExt;

const TEST_CONNECTOR: &str = "test";
const TEST_ACTION_NAME: &str = "echo";

#[tokio::test]
async fn execute_by_trn_rejects_cross_tenant_action() {
    let ctx = TestContext::new(Duration::from_millis(0)).await;
    let payload = json!({"action_trn": ctx.action_trn, "input": {}});

    let response = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/execute")
                .header("content-type", "application/json")
                .header("x-tenant", "tenant-b")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND);
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["success"], serde_json::Value::Bool(false));
    assert_eq!(body_json["error"]["code"], serde_json::Value::String("NOT_FOUND".into()));
}

#[tokio::test]
async fn execute_action_dry_run_skips_execution_and_returns_metadata() {
    let ctx = TestContext::new(Duration::from_millis(0)).await;
    let uri = format!("/api/v1/actions/{}/execute", ctx.tool_name.as_str());
    let payload = json!({
        "input": {"message": "hello"},
        "options": {"dry_run": true}
    });

    let response = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-tenant", ctx.tenant.as_str())
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body_json["success"], serde_json::Value::Bool(true));
    assert_eq!(
        body_json["data"]["result"],
        json!({"dry_run": true, "input": {"message": "hello"}})
    );
    assert_eq!(
        body_json["metadata"]["action_trn"],
        serde_json::Value::String(ctx.action_trn.clone())
    );
    let warnings = body_json["metadata"]["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|w| w.as_str() == Some("dry_run=true")));
    assert_eq!(ctx.executions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn execute_action_timeout_respects_request_override() {
    let ctx = TestContext::new(Duration::from_millis(80)).await;
    let uri = format!("/api/v1/actions/{}/execute", ctx.tool_name.as_str());
    let payload = json!({
        "input": {"slow": true},
        "options": {"timeout_ms": 10}
    });

    let response = ctx
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-tenant", ctx.tenant.as_str())
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(ctx.executions.load(Ordering::SeqCst), 1);
}

struct TestContext {
    router: Router,
    executions: Arc<AtomicUsize>,
    action_trn: String,
    tenant: String,
    tool_name: String,
}

impl TestContext {
    async fn new(action_delay: Duration) -> Self {
        let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
        let executions = Arc::new(AtomicUsize::new(0));

        let tenant = "tenant-a".to_string();
        let connection_trn = Trn::new(format!(
            "trn:openact:{}:connection/{}/conn@v1",
            tenant, TEST_CONNECTOR
        ));
        let action_trn = Trn::new(format!(
            "trn:openact:{}:action/{}/{}@v1",
            tenant, TEST_CONNECTOR, TEST_ACTION_NAME
        ));

        let now = Utc::now();
        let connector = ConnectorKind::new(TEST_CONNECTOR);

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

        ActionRepository::upsert(
            store.as_ref(),
            &ActionRecord {
                trn: action_trn.clone(),
                connector: connector.clone(),
                name: "echo".into(),
                connection_trn: connection_trn.clone(),
                config_json: json!({"input_schema": {"type": "object"}}),
                mcp_enabled: true,
                mcp_overrides: None,
                created_at: now,
                updated_at: now,
                version: 1,
            },
        )
        .await
        .unwrap();

        let actions = ActionRepository::list_by_connector(store.as_ref(), &connector)
            .await
            .unwrap();
        assert_eq!(actions.len(), 1, "expected action to be persisted");

        let conn_store = store.as_ref().clone();
        let act_store = store.as_ref().clone();
        let mut registry = openact_registry::ConnectorRegistry::new(conn_store, act_store);

        let factory = Arc::new(TestFactory {
            executions: executions.clone(),
            delay: action_delay,
        });
        registry.register_connection_factory(factory.clone());
        registry.register_action_factory(factory);

        let app_state = AppState {
            store,
            registry: Arc::new(registry),
        };
        let governance = openact_mcp::GovernanceConfig::new(vec![], vec![], 4, 1);

        let router = create_router(app_state, governance);

        Self {
            router,
            executions,
            action_trn: action_trn.as_str().to_string(),
            tenant,
            tool_name: format!("{}.{}", TEST_CONNECTOR, TEST_ACTION_NAME),
        }
    }
}

struct TestFactory {
    executions: Arc<AtomicUsize>,
    delay: Duration,
}

struct TestConnection {
    trn: Trn,
    connector: ConnectorKind,
    delay: Duration,
}

struct TestAction {
    trn: Trn,
    connector: ConnectorKind,
    executions: Arc<AtomicUsize>,
    delay: Duration,
}

impl openact_registry::factory::AsAny for TestConnection {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[async_trait]
impl openact_registry::factory::Connection for TestConnection {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector
    }

    async fn health_check(&self) -> openact_registry::RegistryResult<bool> {
        Ok(true)
    }

    fn metadata(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait]
impl openact_registry::factory::Action for TestAction {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector
    }

    async fn execute(
        &self,
        input: serde_json::Value,
    ) -> openact_registry::RegistryResult<serde_json::Value> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            sleep(self.delay).await;
        }
        Ok(json!({"echo": input}))
    }

    fn metadata(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait]
impl openact_registry::factory::ConnectionFactory for TestFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new(TEST_CONNECTOR)
    }

    fn metadata(&self) -> openact_core::types::ConnectorMetadata {
        openact_core::types::ConnectorMetadata {
            kind: ConnectorKind::new(TEST_CONNECTOR),
            display_name: "Test Connector".into(),
            description: "Test connector for integration tests".into(),
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
    ) -> openact_registry::RegistryResult<Box<dyn openact_registry::factory::Connection>> {
        Ok(Box::new(TestConnection {
            trn: record.trn.clone(),
            connector: record.connector.clone(),
            delay: self.delay,
        }))
    }
}

#[async_trait]
impl openact_registry::factory::ActionFactory for TestFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new(TEST_CONNECTOR)
    }

    fn metadata(&self) -> openact_core::types::ConnectorMetadata {
        openact_core::types::ConnectorMetadata {
            kind: ConnectorKind::new(TEST_CONNECTOR),
            display_name: "Test Connector".into(),
            description: "Test connector for integration tests".into(),
            category: "test".into(),
            supported_operations: vec![],
            supports_auth: false,
            example_config: None,
            version: "1.0".into(),
        }
    }

    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Box<dyn openact_registry::factory::Connection>,
    ) -> openact_registry::RegistryResult<Box<dyn openact_registry::factory::Action>> {
        let delay = connection
            .as_any()
            .downcast_ref::<TestConnection>()
            .map(|c| c.delay)
            .unwrap_or(self.delay);

        Ok(Box::new(TestAction {
            trn: action_record.trn.clone(),
            connector: action_record.connector.clone(),
            executions: self.executions.clone(),
            delay,
        }))
    }
}
