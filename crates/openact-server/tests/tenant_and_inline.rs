use axum::{
    body::{self, Body},
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use openact_core::{
    orchestration::{OrchestratorOutboxStore, OrchestratorRunStore},
    store::{ActionRepository, ConnectionStore},
    types::{ActionRecord, ConnectionRecord, ConnectorKind, Trn},
};
use openact_server::{
    orchestration::{
        HeartbeatSupervisor, HeartbeatSupervisorConfig, OutboxDispatcher, OutboxDispatcherConfig,
        OutboxService, RunService,
    },
    restapi::create_router,
    AppState,
};
use openact_store::SqlStore;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

async fn make_router() -> Router {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let conn_store = store.as_ref().clone();
    let act_store = store.as_ref().clone();
    let registry = openact_registry::ConnectorRegistry::new(conn_store, act_store);
    let orchestrator_runs: Arc<dyn OrchestratorRunStore> = store.clone();
    let orchestrator_outbox: Arc<dyn OrchestratorOutboxStore> = store.clone();
    let run_service = RunService::new(orchestrator_runs.clone());
    let outbox_service = OutboxService::new(orchestrator_outbox.clone());
    let outbox_dispatcher = Arc::new(OutboxDispatcher::new(
        outbox_service.clone(),
        run_service.clone(),
        "http://localhost:8080/api/v1/stepflow/events".to_string(),
        OutboxDispatcherConfig::default(),
    ));
    let heartbeat_supervisor = Arc::new(HeartbeatSupervisor::new(
        run_service.clone(),
        outbox_service.clone(),
        HeartbeatSupervisorConfig::default(),
    ));

    let app_state = AppState {
        store: store.clone(),
        registry: Arc::new(registry),
        orchestrator_runs,
        orchestrator_outbox,
        run_service,
        outbox_service,
        outbox_dispatcher,
        heartbeat_supervisor,
        #[cfg(feature = "authflow")]
        flow_manager: Arc::new(openact_server::flow_runner::FlowRunManager::new(store.clone())),
    };
    let governance = openact_mcp::GovernanceConfig::new(vec![], vec![], 4, 1);
    create_router().with_state((app_state, governance))
}

#[tokio::test]
async fn tenant_middleware_behaviour() {
    // Provide default via header
    let router = make_router().await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/kinds")
                .header("x-tenant", "default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["metadata"]["tenant"], serde_json::Value::String("default".into()));

    // no env var manipulation to avoid cross-test interference
}

#[tokio::test]
async fn execute_inline_validation_success() {
    std::env::set_var("OPENACT_REQUIRE_TENANT", "0");
    let router = make_router().await;
    let body = json!({
        "tenant": "acme",
        "action": "do_it",
        "connections": [{
            "trn": "trn:openact:acme:connection/http/conn@v1",
            "connector": "http",
            "name": "conn",
            "version": 1,
            "base_url": "https://example.com"
        }],
        "actions": [{
            "trn": "trn:openact:acme:action/http/do_it@v1",
            "connector": "http",
            "name": "do_it",
            "connection_trn": "trn:openact:acme:connection/http/conn@v1",
            "version": 1,
            "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}
        }],
        "input": {"q": "ok"},
        "options": {"validate": true, "dry_run": true}
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/execute-inline")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let warnings = body_json["metadata"]["warnings"].as_array().cloned().unwrap_or_default();
    assert!(warnings.iter().any(|w| w
        .as_str()
        .map(|s| s.starts_with("input_schema_digest=sha256:"))
        .unwrap_or(false)));
    assert!(warnings.iter().any(|w| w.as_str() == Some("validated=true")));
    assert_eq!(body_json["metadata"]["tenant"], serde_json::Value::String("acme".into()));
    assert_eq!(
        body_json["metadata"]["action_trn"],
        serde_json::Value::String("trn:openact:acme:action/http/do_it@v1".into())
    );
}

#[tokio::test]
async fn execute_inline_validation_failure() {
    std::env::set_var("OPENACT_REQUIRE_TENANT", "0");
    let router = make_router().await;
    let body = json!({
        "tenant": "acme",
        "action": "do_it",
        "connections": [{
            "trn": "trn:openact:acme:connection/http/conn@v1",
            "connector": "http",
            "name": "conn",
            "version": 1,
            "base_url": "https://example.com"
        }],
        "actions": [{
            "trn": "trn:openact:acme:action/http/do_it@v1",
            "connector": "http",
            "name": "do_it",
            "connection_trn": "trn:openact:acme:connection/http/conn@v1",
            "version": 1,
            "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}
        }],
        "input": {},
        "options": {"validate": true, "dry_run": true}
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/execute-inline")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["success"], serde_json::Value::Bool(false));
    assert_eq!(body_json["error"]["code"], serde_json::Value::String("INVALID_INPUT".into()));
}

async fn seed_actions(store: &Arc<SqlStore>, tenant: &str) {
    let now = Utc::now();
    // connections
    for (connector, name) in [("http", "conn"), ("postgres", "db")] {
        let ctrn = Trn::new(format!("trn:openact:{}:connection/{}/{name}@v1", tenant, connector));
        let rec = ConnectionRecord {
            trn: ctrn,
            connector: ConnectorKind::new(connector),
            name: name.to_string(),
            config_json: json!({}),
            created_at: now,
            updated_at: now,
            version: 1,
        };
        ConnectionStore::upsert(store.as_ref(), &rec).await.unwrap();
    }
    // actions: http/get, http/post, postgres/query, postgres/delete
    let http_conn_trn = Trn::new(format!("trn:openact:{}:connection/http/conn@v1", tenant));
    let pg_conn_trn = Trn::new(format!("trn:openact:{}:connection/postgres/db@v1", tenant));
    let actions = vec![
        ("http", "get", http_conn_trn.clone()),
        ("http", "post", http_conn_trn.clone()),
        ("postgres", "query", pg_conn_trn.clone()),
        ("postgres", "delete", pg_conn_trn.clone()),
    ];
    for (connector, name, ctrn) in actions {
        let atrn = Trn::new(format!("trn:openact:{}:action/{}/{name}@v1", tenant, connector));
        let rec = ActionRecord {
            trn: atrn,
            connector: ConnectorKind::new(connector),
            name: name.to_string(),
            connection_trn: ctrn,
            config_json: json!({}),
            mcp_enabled: true,
            mcp_overrides: None,
            created_at: now,
            updated_at: now,
            version: 1,
        };
        ActionRepository::upsert(store.as_ref(), &rec).await.unwrap();
    }
}

async fn make_router_with_gov(allow: Vec<&str>, deny: Vec<&str>, seed: bool) -> Router {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    if seed {
        seed_actions(&store, "acme").await;
    }
    let conn_store = store.as_ref().clone();
    let act_store = store.as_ref().clone();
    let registry = openact_registry::ConnectorRegistry::new(conn_store, act_store);
    let orchestrator_runs: Arc<dyn openact_core::orchestration::OrchestratorRunStore> =
        store.clone();
    let orchestrator_outbox: Arc<dyn openact_core::orchestration::OrchestratorOutboxStore> =
        store.clone();
    let run_service = openact_server::orchestration::RunService::new(orchestrator_runs.clone());
    let outbox_service =
        openact_server::orchestration::OutboxService::new(orchestrator_outbox.clone());
    let outbox_dispatcher = Arc::new(openact_server::orchestration::OutboxDispatcher::new(
        outbox_service.clone(),
        run_service.clone(),
        "http://localhost:8080/api/v1/stepflow/events".to_string(),
    ));
    let heartbeat_supervisor = Arc::new(openact_server::orchestration::HeartbeatSupervisor::new(
        run_service.clone(),
        outbox_service.clone(),
    ));

    let app_state = AppState {
        store: store.clone(),
        registry: Arc::new(registry),
        orchestrator_runs,
        orchestrator_outbox,
        run_service,
        outbox_service,
        outbox_dispatcher,
        heartbeat_supervisor,
        #[cfg(feature = "authflow")]
        flow_manager: Arc::new(openact_server::flow_runner::FlowRunManager::new(store.clone())),
    };
    let allow = allow.into_iter().map(|s| s.to_string()).collect();
    let deny = deny.into_iter().map(|s| s.to_string()).collect();
    let governance = openact_mcp::GovernanceConfig::new(allow, deny, 4, 1);
    create_router().with_state((app_state, governance))
}

#[tokio::test]
async fn governance_filter_allow_patterns_total_consistency() {
    // allow http.* -> only http actions (2)
    let router = make_router_with_gov(vec!["http.*"], vec![], true).await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/actions?page=1&page_size=50")
                .header("x-tenant", "acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let actions = body_json["data"]["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(body_json["data"]["total"], json!(2));

    // allow *.get -> only name get (1)
    let router = make_router_with_gov(vec!["*.get"], vec![], true).await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/actions")
                .header("x-tenant", "acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["total"], json!(1));
}

#[tokio::test]
async fn governance_filter_deny_patterns_total_consistency() {
    // deny *.delete -> total = 3
    let router = make_router_with_gov(vec!["*"], vec!["*.delete"], true).await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/actions")
                .header("x-tenant", "acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["total"], json!(3));

    // deny http.* -> only postgres (2)
    let router = make_router_with_gov(vec!["*"], vec!["http.*"], true).await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/actions")
                .header("x-tenant", "acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["total"], json!(2));
}
