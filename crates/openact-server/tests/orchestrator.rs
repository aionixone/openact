use aionix_contracts::{CommandEnvelope, Trn as ContractTrn};
use axum::{body::Body, http::Request};
use chrono::{Duration as ChronoDuration, Utc};
use httpmock::{Method::GET, MockServer};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower::ServiceExt;
use urlencoding::encode;

use aionix_contracts::idempotency::DedupStore;
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorOutboxStore, OrchestratorRunStatus, OrchestratorRunStore,
};
use openact_core::types::Trn;
use openact_server::orchestration::{
    AsyncTaskManager, HeartbeatSupervisor, HeartbeatSupervisorConfig, OutboxDispatcher,
    OutboxDispatcherConfig, OutboxService, RunService, StepflowCommandAdapter,
};
use openact_server::{restapi::create_router, AppState};
use openact_store::SqlStore;
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

fn sample_envelope(target: &Trn, tenant: &str) -> (CommandEnvelope, String, String) {
    let run_uuid = Uuid::new_v4().to_string();
    let run_id = format!("trn:stepflow:{}:execution/demo/{}@v1", tenant, run_uuid);
    let state_name = "SampleState".to_string();

    let mut parameters = Map::new();
    parameters.insert("runId".to_string(), Value::String(run_id.clone()));
    parameters.insert("stateName".to_string(), Value::String(state_name.clone()));

    let mut extensions = Map::new();
    extensions.insert("runTrn".to_string(), Value::String(run_id.clone()));
    extensions.insert("stateName".to_string(), Value::String(state_name.clone()));

    let envelope = CommandEnvelope {
        schema_version: "1.1.0".to_string(),
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        command: "openact.test.execute".to_string(),
        source: ContractTrn::parse(&format!("trn:stepflow:{}:engine", tenant)).unwrap(),
        target: ContractTrn::parse(target.as_str()).unwrap(),
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

#[derive(Default)]
struct RecordingDedupStore {
    seen: Mutex<HashSet<String>>,
    duplicates: AtomicUsize,
}

impl RecordingDedupStore {
    fn duplicate_count(&self) -> usize {
        self.duplicates.load(Ordering::SeqCst)
    }
}

impl DedupStore for RecordingDedupStore {
    fn check_and_record(&self, key: &str) -> bool {
        let mut guard = self.seen.lock().expect("dedup mutex poisoned");
        if guard.insert(key.to_string()) {
            false
        } else {
            self.duplicates.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    fn remove(&self, key: &str) {
        if let Ok(mut guard) = self.seen.lock() {
            guard.remove(key);
        }
    }
}

fn expected_task_trn(run_id: &str, state: &str) -> String {
    let parsed = ContractTrn::parse(run_id).unwrap();
    let run_path = parsed.resource_path().unwrap_or("");
    let version = parsed.version().unwrap_or("v1");
    format!("trn:stepflow:{}:task/{}/{}@{}", parsed.tenant(), run_path, state, version)
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
        None,
    ));
    let heartbeat_supervisor = Arc::new(HeartbeatSupervisor::new(
        run_service.clone(),
        outbox_service.clone(),
        HeartbeatSupervisorConfig::default(),
    ));
    let async_manager =
        Arc::new(AsyncTaskManager::new(run_service.clone(), outbox_service.clone()));

    AppState {
        store,
        registry: Arc::new(registry),
        orchestrator_runs,
        orchestrator_outbox,
        run_service,
        outbox_service,
        outbox_dispatcher,
        heartbeat_supervisor,
        async_manager,
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
    assert_eq!(payload["resourceTrn"], Value::String(expected_task_trn(&run_id, &expected_state)));
    assert_eq!(payload["data"]["stateName"], Value::String(expected_state.clone()));
    assert_eq!(payload["runId"], Value::String(run_id));
    assert!(payload.get("taskRunId").and_then(|v| v.as_str()).is_some());
    assert!(payload.get("outboxMessageId").and_then(|v| v.as_str()).is_some());
}

#[tokio::test]
async fn async_task_manager_mock_complete() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, _, _) = sample_envelope(&target_trn, tenant);
    let (run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    let run_id = run_record.run_id.clone();
    app_state.run_service.create_run(run_record.clone()).await.unwrap();

    let handle = json!({
        "backendId": "generic_async",
        "externalRunId": "mock-123",
        "config": {
            "tracker": {
                "kind": "mock_complete",
                "delay_ms": 25,
                "result": {"value": 42}
            }
        }
    });

    app_state.async_manager.submit(run_record, handle).unwrap();

    // wait for background completion
    let mut attempts = 0;
    loop {
        attempts += 1;
        let current = app_state.run_service.get(&run_id).await.unwrap().unwrap();
        if current.status == OrchestratorRunStatus::Succeeded {
            assert_eq!(current.result.unwrap(), json!({"value": 42}));
            break;
        }
        assert!(attempts < 50, "async task manager did not complete in time");
        sleep(TokioDuration::from_millis(20)).await;
    }

    let outbox =
        app_state.outbox_service.fetch_due(Utc::now(), 10).await.expect("fetch due outbox");
    assert_eq!(outbox.len(), 1, "expected success event enqueued");
}

#[tokio::test]
#[ignore]
async fn async_task_manager_http_poll_success() {
    let server = MockServer::start_async().await;
    let mock = server.mock(|when, then| {
        when.method(GET).path("/status/mock-123");
        then.status(200).json_body(json!({ "result": { "value": "done" } }));
    });

    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, _, _) = sample_envelope(&target_trn, tenant);
    let (mut run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    let run_id = run_record.run_id.clone();
    run_record.metadata = Some(json!({
        "asyncHandle": {
            "externalRunId": "mock-123",
            "config": {
                "tracker": {
                    "kind": "http_poll",
                    "url": server.url("/status/{{externalRunId}}"),
                    "method": "GET",
                    "interval_ms": 20,
                    "timeout_ms": 2000,
                    "success_status": [200],
                    "result_pointer": "/result"
                }
            }
        },
        "asyncMode": "running"
    }));
    run_record.external_ref = Some("mock-123".to_string());
    app_state.run_service.create_run(run_record.clone()).await.unwrap();

    let handle = json!({
        "backendId": "generic_async",
        "externalRunId": "mock-123",
        "config": {
            "tracker": {
                "kind": "http_poll",
                "url": server.url("/status/{{externalRunId}}"),
                "method": "GET",
                "interval_ms": 20,
                "timeout_ms": 2000,
                "success_status": [200],
                "result_pointer": "/result"
            }
        }
    });

    app_state.async_manager.submit(run_record, handle).unwrap();

    let mut attempts = 0;
    let final_record = loop {
        attempts += 1;
        let current = app_state.run_service.get(&run_id).await.unwrap().unwrap();
        if current.status == OrchestratorRunStatus::Succeeded {
            break current;
        }
        assert!(attempts < 200, "http poll waiter did not complete in time");
        sleep(TokioDuration::from_millis(20)).await;
    };

    assert_eq!(final_record.result, Some(json!({"value": "done"})));
    assert_eq!(final_record.external_ref, Some("mock-123".to_string()));
    let metadata = final_record.metadata.unwrap();
    assert_eq!(metadata.get("asyncMode").and_then(|v| v.as_str()), Some("running"));
    assert!(metadata.get("asyncHandle").is_some());

    assert!(mock.hits() >= 1, "expected at least one poll request, observed {}", mock.hits());

    let outbox =
        app_state.outbox_service.fetch_due(Utc::now(), 10).await.expect("fetch due outbox");
    assert_eq!(outbox.len(), 1, "expected success event enqueued");
}

#[tokio::test]
async fn outbox_dispatcher_skips_duplicate_events() {
    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let dedup_store = Arc::new(RecordingDedupStore::default());
    let dedup_arc: Arc<dyn DedupStore> = dedup_store.clone();

    let orchestrator_runs: Arc<dyn OrchestratorRunStore> = store.clone();
    let orchestrator_outbox: Arc<dyn OrchestratorOutboxStore> = store.clone();
    let run_service = RunService::new(orchestrator_runs.clone());
    let outbox_service = OutboxService::new(orchestrator_outbox.clone());

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, _, _) = sample_envelope(&target_trn, tenant);
    let (run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    run_service.create_run(run_record.clone()).await.expect("persist run");

    let success_event =
        StepflowCommandAdapter::build_success_event(&run_record, &json!({"ok": true}));
    let payload_value = serde_json::to_value(&success_event).expect("event to value");

    outbox_service
        .enqueue(OrchestratorOutboxInsert {
            run_id: Some(run_record.run_id.clone()),
            protocol: "aionix.event.stepflow".to_string(),
            payload: payload_value.clone(),
            next_attempt_at: Utc::now(),
            attempts: 0,
            last_error: None,
        })
        .await
        .expect("enqueue event");

    let due = outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .expect("fetch due");
    assert_eq!(due.len(), 1);

    let dispatcher = OutboxDispatcher::with_client(
        outbox_service.clone(),
        run_service.clone(),
        String::new(),
        OutboxDispatcherConfig::default(),
        None,
        Some(dedup_arc.clone()),
    );

    dispatcher.process_batch_once().await.expect("dispatch event");
    assert_eq!(dedup_store.duplicate_count(), 0);

    outbox_service
        .enqueue(OrchestratorOutboxInsert {
            run_id: Some(run_record.run_id.clone()),
            protocol: "aionix.event.stepflow".to_string(),
            payload: payload_value,
            next_attempt_at: Utc::now(),
            attempts: 0,
            last_error: None,
        })
        .await
        .expect("enqueue duplicate");

    let second_due = outbox_service
        .fetch_due(Utc::now() + ChronoDuration::seconds(1), 10)
        .await
        .expect("fetch duplicate due");
    assert_eq!(second_due.len(), 1);

    dispatcher.process_batch_once().await.expect("dedup skip");
    assert_eq!(dedup_store.duplicate_count(), 1);
}

#[tokio::test]
#[ignore]
async fn async_task_manager_cancel_plan() {
    let server = MockServer::start_async().await;
    let mock = server.mock(|when, then| {
        when.method(GET).path("/cancel/mock-321");
        then.status(200);
    });

    let store = Arc::new(SqlStore::new("sqlite::memory:").await.unwrap());
    let app_state = build_app_state(store.clone()).await;

    let tenant = "acme";
    let target_trn = Trn::new("trn:openact:acme:action/http/test".to_string());
    let (envelope, _, _) = sample_envelope(&target_trn, tenant);
    let (mut run_record, _) = StepflowCommandAdapter::prepare_run(
        &envelope,
        tenant,
        &target_trn,
        Duration::from_secs(30),
    );
    run_record.metadata = Some(json!({
        "asyncHandle": {
            "externalRunId": "mock-321",
            "config": {
                "cancel": {
                    "kind": "http",
                    "url": server.url("/cancel/{{externalRunId}}"),
                    "method": "GET"
                }
            }
        }
    }));
    app_state.run_service.create_run(run_record.clone()).await.unwrap();

    let handle = run_record.metadata.as_ref().and_then(|v| v.get("asyncHandle")).cloned().unwrap();

    app_state.async_manager.cancel_run(&run_record, &handle, Some("user_request")).await.unwrap();

    assert!(mock.hits() >= 1);
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
    assert_eq!(payload["resourceTrn"], Value::String(expected_task_trn(&run_id, &expected_state)));
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
    assert_eq!(payload["resourceTrn"], Value::String(expected_task_trn(&run_id, &expected_state)));
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
    assert_eq!(payload["resourceTrn"], Value::String(expected_task_trn(&run_id, &expected_state)));
}
