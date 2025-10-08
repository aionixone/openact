#![cfg(feature = "authflow")]

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use openact_authflow::{
    actions::ActionRouter,
    engine::TaskHandler,
    runner::{FlowRunResult, FlowRunner, FlowRunnerConfig},
};
use openact_core::store::{AuthConnectionStore, RunStore};
use openact_store::SqlStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stepflow_dsl::WorkflowDSL;
use tokio::sync::RwLock;
use tracing::warn;

/// In-memory manager for AuthFlow runs triggered via REST.
#[derive(Clone)]
pub struct FlowRunManager {
    store: Arc<SqlStore>,
    handler: Arc<ActionRouter>,
    runs: Arc<RwLock<HashMap<String, FlowRunRecord>>>,
}

impl FlowRunManager {
    pub fn new(store: Arc<SqlStore>) -> Self {
        let auth_store: Arc<dyn AuthConnectionStore> = store.clone();
        let handler = Arc::new(ActionRouter::new(auth_store));
        Self { store, handler, runs: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Start a new flow run and track its status.
    pub async fn start(
        &self,
        dsl: Arc<WorkflowDSL>,
        config: FlowRunnerConfig,
        input: Value,
        tenant: String,
    ) -> anyhow::Result<FlowRunRecord> {
        let handler: Arc<dyn TaskHandler> = self.handler.clone();
        let run_store: Arc<dyn RunStore> = self.store.clone();
        let runner = FlowRunner::new(dsl, handler, run_store, config);
        let handle = runner.start(input).await?;

        let run_id = handle.run_id().to_string();
        let state_token = handle.state_token().to_string();
        let authorize_url = handle.authorize_url.clone();
        let callback_url = handle.callback_url.clone();

        let now = Utc::now();
        let initial = FlowRunRecord {
            run_id: run_id.clone(),
            tenant,
            authorize_url,
            callback_url,
            state_token,
            status: FlowRunStatus::Pending,
            error: None,
            auth_ref: None,
            connection_ref: None,
            final_context: None,
            started_at: now,
            updated_at: now,
        };

        {
            let mut runs = self.runs.write().await;
            runs.insert(initial.run_id.clone(), initial.clone());
        }

        let runs = Arc::clone(&self.runs);
        let run_id_for_task = run_id.clone();
        tokio::spawn(async move {
            let outcome = handle.wait_for_completion().await;
            let mut runs = runs.write().await;
            if let Some(record) = runs.get_mut(&run_id_for_task) {
                record.updated_at = Utc::now();
                match outcome {
                    Ok(result) => apply_success(record, result),
                    Err(err) => {
                        record.status = FlowRunStatus::Failed;
                        record.error = Some(err.to_string());
                    }
                }
            } else {
                warn!(run_id = %run_id_for_task, "flow run disappeared before completion");
            }
        });

        Ok(initial)
    }

    pub async fn get(&self, run_id: &str) -> Option<FlowRunRecord> {
        let runs = self.runs.read().await;
        runs.get(run_id).cloned()
    }
}

fn apply_success(record: &mut FlowRunRecord, result: FlowRunResult) {
    record.status = FlowRunStatus::Completed;
    record.error = None;
    record.auth_ref = result.auth_ref;
    record.connection_ref = result.connection_ref;
    record.final_context = Some(result.final_context);
}

/// Snapshot of a flow run maintained in memory (per process).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowRunRecord {
    pub run_id: String,
    pub tenant: String,
    pub authorize_url: String,
    pub callback_url: String,
    pub state_token: String,
    pub status: FlowRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_context: Option<Value>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Lifecycle state of a flow run.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowRunStatus {
    Pending,
    Completed,
    Failed,
}
