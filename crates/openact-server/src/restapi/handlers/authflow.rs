#![cfg(feature = "authflow")]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use openact_authflow::runner::FlowRunnerConfig;
use openact_mcp::GovernanceConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stepflow_dsl::WorkflowDSL;

use crate::{
    dto::{ResponseEnvelope, ResponseMeta},
    error::ServerError,
    flow_runner::{FlowRunRecord, FlowRunStatus},
    middleware::{request_id::RequestId, tenant::Tenant},
    AppState,
};

#[derive(Deserialize)]
pub struct FlowRunStartRequest {
    pub dsl: Value,
    #[serde(default = "default_input_value")]
    pub input: Value,
    #[serde(default)]
    pub authorize_url_ptr: Option<String>,
    #[serde(default)]
    pub state_ptr: Option<String>,
    #[serde(default)]
    pub redirect_ptr: Option<String>,
    #[serde(default)]
    pub auth_ref_ptr: Option<String>,
    #[serde(default)]
    pub connection_ref_ptr: Option<String>,
    #[serde(default)]
    pub callback_addr: Option<String>,
    #[serde(default)]
    pub callback_path: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

fn default_input_value() -> Value {
    Value::Object(Default::default())
}

#[derive(Serialize)]
pub struct FlowRunView {
    pub run_id: String,
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
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<FlowRunRecord> for FlowRunView {
    fn from(record: FlowRunRecord) -> Self {
        Self {
            run_id: record.run_id,
            authorize_url: record.authorize_url,
            callback_url: record.callback_url,
            state_token: record.state_token,
            status: record.status,
            error: record.error,
            auth_ref: record.auth_ref,
            connection_ref: record.connection_ref,
            final_context: record.final_context,
            started_at: record.started_at,
            updated_at: record.updated_at,
        }
    }
}

pub async fn start_flow_run(
    State((app_state, _governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
    Json(payload): Json<FlowRunStartRequest>,
) -> Result<Json<ResponseEnvelope<FlowRunView>>, (StatusCode, Json<crate::error::ErrorResponse>)> {
    let request_id_str = request_id.0.clone();
    let FlowRunStartRequest {
        dsl,
        input,
        authorize_url_ptr,
        state_ptr,
        redirect_ptr,
        auth_ref_ptr,
        connection_ref_ptr,
        callback_addr,
        callback_path,
        timeout_secs,
    } = payload;

    let dsl: WorkflowDSL = serde_json::from_value(dsl)
        .map_err(|e| ServerError::InvalidInput(format!("Invalid workflow DSL: {}", e)))
        .map_err(|e| e.to_http_response(request_id_str.clone()))?;
    let dsl = Arc::new(dsl);

    let mut config = FlowRunnerConfig::default();
    if let Some(ptr) = authorize_url_ptr {
        config.authorize_url_ptr = ptr;
    }
    if let Some(ptr) = state_ptr {
        config.state_ptr = ptr;
    }
    if let Some(ptr) = redirect_ptr {
        if ptr.trim().is_empty() {
            config.redirect_ptr = None;
        } else {
            config.redirect_ptr = Some(ptr);
        }
    }
    if let Some(ptr) = auth_ref_ptr {
        if ptr.trim().is_empty() {
            config.auth_ref_ptr = None;
        } else {
            config.auth_ref_ptr = Some(ptr);
        }
    }
    if let Some(ptr) = connection_ref_ptr {
        if ptr.trim().is_empty() {
            config.connection_ref_ptr = None;
        } else {
            config.connection_ref_ptr = Some(ptr);
        }
    }
    if let Some(addr) = callback_addr {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e| ServerError::InvalidInput(format!("Invalid callback_addr: {}", e)))
            .map_err(|e| e.to_http_response(request_id_str.clone()))?;
        config.callback_addr = parsed;
    }
    if let Some(path) = callback_path {
        config.callback_path = path;
    }
    if let Some(timeout) = timeout_secs {
        config.callback_timeout = Duration::from_secs(timeout);
    }

    let record = app_state
        .flow_manager
        .start(dsl, config, input, tenant.as_str().to_string())
        .await
        .map_err(|e| ServerError::InvalidInput(e.to_string()))
        .map_err(|e| e.to_http_response(request_id_str.clone()))?;

    let response = ResponseEnvelope {
        success: true,
        data: FlowRunView::from(record),
        metadata: ResponseMeta {
            request_id: request_id_str,
            tenant: Some(tenant.as_str().to_string()),
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}

pub async fn get_flow_run(
    State((app_state, _governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
    Path(run_id): Path<String>,
) -> Result<Json<ResponseEnvelope<FlowRunView>>, (StatusCode, Json<crate::error::ErrorResponse>)> {
    let request_id_str = request_id.0.clone();
    let tenant_name = tenant.as_str().to_string();
    let record = match app_state.flow_manager.get(&run_id).await {
        Some(rec) if rec.tenant == tenant_name => rec,
        Some(_) => {
            let err = ServerError::NotFound("Flow run not found".into());
            return Err(err.to_http_response(request_id_str.clone()));
        }
        None => {
            let err = ServerError::NotFound("Flow run not found".into());
            return Err(err.to_http_response(request_id_str.clone()));
        }
    };

    let response = ResponseEnvelope {
        success: true,
        data: FlowRunView::from(record),
        metadata: ResponseMeta {
            request_id: request_id_str,
            tenant: Some(tenant_name),
            execution_time_ms: None,
            action_trn: None,
            version: None,
            warnings: None,
        },
    };

    Ok(Json(response))
}
