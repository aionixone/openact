use axum::{body::Body, http::Request};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use urlencoding::encode;

use openact_core::orchestration::{
    OrchestratorOutboxStore, OrchestratorRunStatus, OrchestratorRunStore,
};
use openact_core::types::Trn;
use openact_server::orchestration::{
    HeartbeatSupervisor, HeartbeatSupervisorConfig, OutboxDispatcher, OutboxDispatcherConfig,
    OutboxService, RunService, StepflowCommandAdapter,
};
use openact_server::{restapi::create_router, AppState};
use openact_store::SqlStore;
use uuid::Uuid;

fn sample_envelope(
    target: &Trn,
    tenant: &str,
) -> (aionix_protocol::CommandEnvelope, String, String) {
    let run_uuid = Uuid::new_v4().to_string();
    let run_id = format!("trn:stepflow:{}:execution/demo:{}", tenant, run_uuid);
    let state_name = "SampleState".to_string();

    let mut parameters = Map::new();
    parameters.insert("runId".to_string(), Value::String(run_id.clone()));
    parameters.insert("stateName".to_string(), Value::String(state_name.clone()));

    let mut extensions = Map::new();
    extensions.insert("runTrn".to_string(), Value::String(run_id.clone()));
    extensions.insert("stateName".to_string(), Value::String(state_name.clone()));

    let envelope = aionix_protocol::CommandEnvelope {
        schema_version: "1.1.0".to_string(),
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        command: "openact.test.execute".to_string(),
        source: format!("trn:stepflow:{}:engine", tenant),
        target: target.as_str().to_string(),
        tenant: tenant.to_string(),
        trace_id: Uuid::new_v4().to_string(),
        parameters,
        actor_trn: None,
        parameters_schema_ref: None,
        expect_response: None,
        timeout_seconds: Some(30),
        idempotency_key: None,
        authz_scopes: None,
        correlation_id: None,
        attachments: None,
        labels: None,
        schedule_at: None,
        deadline: None,
        extensions,
    };

    (envelope, run_id, state_name)
}

async fn build_app_state(store: Arc<SqlStore>) -> AppState {
    let conn_store = store.as_ref().clone();
    let act_store = store.as_ref().clone();
    let registry = openact_registry::ConnectorRegistry::new(conn_store, act_store);

    let orchestrator_runs: Arc<dyn OrchestratorRunStore> = store.clone();
    let orchestrator_outbox: Arc<dyn OrchestratorOutboxStore> = store.clone();
    let run_service = RunService::new(orchestrator_runs.clone());
    let outbox_service = OutboxService::new(orchestrator_outbox.clone());
    let outbox_dispatcher = Arc::new(OutboxDispatcher::with_client(
        outbox_service.clone(),
        run_service.clone(),
        String::new(),
        OutboxDispatcherConfig::default(),
        None,
    ));
    let heartbeat_supervisor = Arc::new(HeartbeatSupervisor::new(
        run_service.clone(),
        outbox_service.clone(),
        HeartbeatSupervisorConfig::default(),
    ));

    AppState {
        store,
        registry: Arc::new(registry),
        orchestrator_runs,
        orchestrator_outbox,
        run_service,
        outbox_service,
        outbox_dispatcher,
        heartbeat_supervisor,
        #[cfg(feature = "authflow")]
        flow_manager: Arc::new(openact_server::flow_runner::FlowRunManager::new(store.clone())),
    }
}

#[tokio::test]
async fn orchestrator_callback_marks_success() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, expected_run_id, expected_state) = sample_envelope(&target_trn, tenant);
    let (run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    let run_id = run_record.run_id.clone();
    assert_eq!(run_id, expected_run_id);
    app_state.run_service.create_run(run_record).await.unwrap();

    let governance = openact_mcp::GovernanceConfig::new(vec![], vec![], 10, 30);
    let router = create_router().with_state((app_state.clone(), governance));

    let encoded_run = encode(&run_id);
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/orchestrator/runs/{}/completion", encoded_run))
        .header("content-type", "application/json")
        .header("x-tenant", tenant)
        .body(Body::from(r#"{"status":"succeeded","result":{"foo":"bar"}}"#))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let run_after = app_state.run_service.get(&run_id).await.unwrap().unwrap();
    assert_eq!(run_after.status, OrchestratorRunStatus::Succeeded);
    assert_eq!(run_after.phase.as_deref(), Some("succeeded"));
    assert_eq!(run_after.result, Some(json!({"foo":"bar"})));

    let due = app_state
        .outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    let payload = &due[0].payload;
    assert_eq!(payload["data"]["status"], "succeeded");
    assert_eq!(payload["type"], "aionix.stepflow.task.succeeded");
    assert_eq!(
        payload["resourceTrn"],
        Value::String(format!("trn:stepflow:{}:task/{}/{}", tenant, run_id, expected_state))
    );
    assert_eq!(payload["data"]["stateName"], Value::String(expected_state.clone()));
    assert_eq!(payload["runId"], Value::String(run_id));
}

#[tokio::test]
async fn orchestrator_callback_marks_failure() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, expected_run_id, expected_state) = sample_envelope(&target_trn, tenant);
    let (run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    let run_id = run_record.run_id.clone();
    assert_eq!(run_id, expected_run_id);
    app_state.run_service.create_run(run_record).await.unwrap();

    let governance = openact_mcp::GovernanceConfig::new(vec![], vec![], 10, 30);
    let router = create_router().with_state((app_state.clone(), governance));

    let encoded_run = encode(&run_id);
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/orchestrator/runs/{}/completion", encoded_run))
        .header("content-type", "application/json")
        .header("x-tenant", tenant)
        .body(Body::from(r#"{"status":"failed","error":{"code":"E_TEST"}}"#))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let run_after = app_state.run_service.get(&run_id).await.unwrap().unwrap();
    assert_eq!(run_after.status, OrchestratorRunStatus::Failed);
    assert_eq!(run_after.phase.as_deref(), Some("failed"));
    assert_eq!(run_after.error, Some(json!({"code":"E_TEST"})));

    let due = app_state
        .outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    let payload = &due[0].payload;
    assert_eq!(payload["data"]["status"], "failed");
    assert_eq!(payload["type"], "aionix.stepflow.task.failed");
    assert_eq!(payload["data"]["stateName"], Value::String(expected_state.clone()));
    assert_eq!(payload["runId"], Value::String(run_id.clone()));
    assert_eq!(
        payload["resourceTrn"],
        Value::String(format!("trn:stepflow:{}:task/{}/{}", tenant, run_id, expected_state))
    );
}

#[tokio::test]
async fn orchestrator_cancel_marks_cancelled() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, expected_run_id, expected_state) = sample_envelope(&target_trn, tenant);
    let (run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    let run_id = run_record.run_id.clone();
    assert_eq!(run_id, expected_run_id);
    app_state.run_service.create_run(run_record).await.unwrap();

    let governance = openact_mcp::GovernanceConfig::new(vec![], vec![], 10, 30);
    let router = create_router().with_state((app_state.clone(), governance));

    let encoded_run = encode(&run_id);
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/stepflow/commands/{}/cancel", encoded_run))
        .header("content-type", "application/json")
        .header("x-tenant", tenant)
        .body(Body::from(r#"{"reason":"cancelled_by_test","requestedBy":"stepflow"}"#))
        .unwrap();

    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);

    let run_after = app_state.run_service.get(&run_id).await.unwrap().unwrap();
    assert_eq!(run_after.status, OrchestratorRunStatus::Cancelled);
    assert_eq!(run_after.phase.as_deref(), Some("cancelled"));

    let due = app_state
        .outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    let payload = &due[0].payload;
    assert_eq!(payload["data"]["status"], "cancelled");
    assert_eq!(payload["type"], "aionix.stepflow.task.cancelled");
    assert_eq!(payload["runId"], Value::String(run_id.clone()));
    assert_eq!(
        payload["resourceTrn"],
        Value::String(format!("trn:stepflow:{}:task/{}/{}", tenant, run_id, expected_state))
    );
}

#[tokio::test]
async fn heartbeat_supervisor_marks_timeout_and_enqueues_event() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, expected_run_id, expected_state) = sample_envelope(&target_trn, tenant);
    let (mut run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    run_record.heartbeat_at = Utc::now() - ChronoDuration::seconds(60);
    run_record.deadline_at = Some(Utc::now() - ChronoDuration::seconds(30));
    let run_id = run_record.run_id.clone();
    assert_eq!(run_id, expected_run_id);
    app_state.run_service.create_run(run_record).await.unwrap();

    app_state.heartbeat_supervisor.process_timeouts_once().await.expect("heartbeat processing");

    let run_after = app_state.run_service.get(&run_id).await.unwrap().unwrap();
    assert_eq!(run_after.status, OrchestratorRunStatus::TimedOut);
    assert_eq!(run_after.phase.as_deref(), Some("timed_out"));

    let due = app_state
        .outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    let payload = &due[0].payload;
    assert_eq!(payload["data"]["status"], "timed_out");
    assert_eq!(payload["type"], "aionix.stepflow.task.timed_out");
    assert_eq!(payload["data"]["stateName"], Value::String(expected_state.clone()));
    assert_eq!(payload["runId"], Value::String(run_id.clone()));
    assert_eq!(
        payload["resourceTrn"],
        Value::String(format!("trn:stepflow:{}:task/{}/{}", tenant, run_id, expected_state))
    );
}
