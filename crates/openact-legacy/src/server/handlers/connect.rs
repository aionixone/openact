#![cfg(feature = "server")]

use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
#[allow(unused_imports)] // Used in utoipa path examples
use serde_json::json;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use crate::app::service::OpenActService;
use crate::interface::dto::{AdhocExecuteRequestDto, ExecuteResponseDto};
use crate::interface::error::helpers;
use crate::store::ConnectionStore;
use crate::templates::TemplateInputs;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ConnectMode {
    Cc,
    Ac,
    DeviceCode,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConnectRequest {
    pub provider: String,
    pub template: String,
    pub tenant: String,
    pub name: String,
    /// Optional OAuth flow DSL (YAML). Required for AC mode.
    #[serde(default)]
    pub dsl_yaml: Option<String>,
    #[serde(default)]
    pub secrets: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub inputs: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub overrides: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub mode: ConnectMode,
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConnectAcStartResponse {
    pub connection_trn: String,
    pub run_id: String,
    pub authorize_url: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_hints: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConnectResult {
    pub connection: crate::models::ConnectionConfig,
    pub status: Option<crate::interface::dto::ConnectionStatusDto>,
    pub test: Option<ExecuteResponseDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_hints: Option<Vec<String>>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connect",
    tag = "connect",
    operation_id = "connect_one_click",
    summary = "One-click connect",
    description = "Create a connection and initiate OAuth flow in one step",
    request_body = ConnectRequest,
    responses(
        (status = 200, description = "OAuth flow initiated successfully", body = ConnectAcStartResponse,
            examples(
                ("authorization_code_flow" = (summary = "Authorization Code flow initiated", value = json!({
                    "run_id": "run_abcd1234",
                    "authorization_url": "https://github.com/login/oauth/authorize?client_id=xyz&state=abcd1234&scope=repo",
                    "next_hints": [
                        "Open authorization URL in browser",
                        "Complete authorization to obtain access token",
                        "Poll status or wait for callback"
                    ]
                }))),
                ("client_credentials_flow" = (summary = "Client Credentials flow completed", value = json!({
                    "message": "Connection created and authenticated successfully",
                    "connection_trn": "trn:openact:my-tenant:connection/github-api@v1",
                    "auth_trn": "trn:openact:my-tenant:auth_connection/auth_xyz789",
                    "test_result": {
                        "status": "success",
                        "response_code": 200,
                        "test_endpoint": "/user"
                    },
                    "next_hints": [
                        "Connection ready to use",
                        "Create tasks using this connection",
                        "Test connection with ad-hoc requests"
                    ]
                })))
            )
        ),
        (status = 400, description = "Invalid request or unsupported connect mode", body = crate::interface::error::ApiError,
            examples(
                ("invalid_mode" = (summary = "Unsupported connect mode", value = json!({
                    "error_code": "validation.invalid_input",
                    "message": "Unsupported connect mode: 'custom'",
                    "hints": ["Supported modes: 'cc' (Client Credentials), 'ac' (Authorization Code)"]
                }))),
                ("missing_secrets" = (summary = "Missing required secrets", value = json!({
                    "error_code": "validation.invalid_input",
                    "message": "Missing required secrets for OAuth configuration",
                    "hints": ["Provide client_id and client_secret", "Check template requirements"]
                }))),
                ("invalid_template" = (summary = "Invalid provider template", value = json!({
                    "error_code": "validation.invalid_input",
                    "message": "Provider template 'unknown' not found",
                    "hints": ["Check available providers", "Use valid provider template"]
                })))
            )
        ),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError,
            examples(
                ("template_load_failed" = (summary = "Template loading failed", value = json!({
                    "error_code": "internal.storage_error",
                    "message": "Failed to load provider template",
                    "hints": ["Check template file exists", "Verify template format"]
                }))),
                ("oauth_init_failed" = (summary = "OAuth initialization failed", value = json!({
                    "error_code": "internal.execution_failed",
                    "message": "Failed to initialize OAuth flow",
                    "hints": ["Check OAuth provider configuration", "Verify network connectivity"]
                })))
            )
        )
    )
))]
pub async fn connect(
    State(svc): State<OpenActService>,
    Json(req): Json<ConnectRequest>,
) -> impl IntoResponse {
    // Build template inputs
    let mut ti = TemplateInputs::default();
    if let Some(s) = req.secrets {
        ti.secrets = s;
    }
    if let Some(i) = req.inputs {
        ti.inputs = i;
    }
    if let Some(o) = req.overrides {
        ti.overrides = o;
    }

    // Create connection from template
    let connection = match svc
        .instantiate_and_upsert_connection(&req.provider, &req.template, &req.tenant, &req.name, ti)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return helpers::validation_error("connect_create_failed", e.to_string())
                .into_response();
        }
    };

    match req.mode {
        ConnectMode::Cc => {
            // Proactively fetch CC token
            if let Err(e) = crate::oauth::runtime::get_cc_token(&connection.trn).await {
                tracing::warn!(target = "connect", trn=%connection.trn, err=%e, "cc token acquisition failed");
            }
            // Status and test
            let status = svc.connection_status(&connection.trn).await.ok().flatten();
            let test = {
                let ep = req
                    .endpoint
                    .unwrap_or_else(|| "https://httpbin.org/get".to_string());
                let dto = AdhocExecuteRequestDto {
                    connection_trn: connection.trn.clone(),
                    method: "GET".to_string(),
                    endpoint: ep,
                    headers: None,
                    query: None,
                    body: None,
                    timeout_config: None,
                    network_config: None,
                    http_policy: None,
                    response_policy: None,
                    retry_policy: None,
                };
                match svc.execute_adhoc(dto).await {
                    Ok(res) => Some(ExecuteResponseDto {
                        status: res.status,
                        headers: res.headers,
                        body: res.body,
                    }),
                    Err(_) => None,
                }
            };
            // Build simple next-step hints
            let mut hints: Vec<String> = Vec::new();
            if let Some(ref s) = status {
                match s.status.as_str() {
                    "ready" => {
                        if let Some(ref t) = test {
                            if t.status < 400 {
                                hints.push("Connection ready: run tasks".to_string());
                            } else {
                                hints.push("Check connection status or fix auth".to_string());
                            }
                        } else {
                            hints.push("Run a test call".to_string());
                        }
                    }
                    "misconfigured" => hints.push("Fix auth parameters and retry".to_string()),
                    "not_issued" => hints.push("Execute once to obtain token".to_string()),
                    "unbound" => hints.push("Bind authorization first".to_string()),
                    _ => hints.push("Check connection status".to_string()),
                }
            } else {
                hints.push("Check connection status".to_string());
            }
            Json(serde_json::json!(ConnectResult {
                connection,
                status,
                test,
                next_hints: Some(hints)
            }))
            .into_response()
        }
        ConnectMode::Ac => {
            // Require caller-provided DSL to ensure correct provider settings
            let Some(dsl_yaml) = req.dsl_yaml.as_deref() else {
                return helpers::validation_error(
                    "dsl_required",
                    "AC mode requires dsl_yaml in request body",
                )
                .with_hints([
                    "Provide dsl_yaml with provider OAuth settings",
                    "See templates for examples",
                ])
                .into_response();
            };
            let dsl: stepflow_dsl::WorkflowDSL = match serde_yaml::from_str(dsl_yaml) {
                Ok(d) => d,
                Err(e) => {
                    return helpers::validation_error("dsl_error", e.to_string())
                        .with_hints([
                            "Ensure YAML is valid",
                            "Check required fields: authorizeUrl/clientId/redirectUri/scope",
                        ])
                        .into_response();
                }
            };
            let router = crate::authflow::actions::DefaultRouter; // not Default
            let run_store = connect_run_store();
            // Execute with empty context; DSL should embed provider params explicitly
            let start = match crate::authflow::workflow::start_obtain(
                &dsl,
                &router,
                run_store,
                serde_json::json!({}),
            ) {
                Ok(s) => s,
                Err(e) => {
                    return helpers::execution_error(e.to_string())
                        .with_hints([
                            "Verify client credentials and redirect URI",
                            "Check DSL mapping for inputs",
                        ])
                        .into_response();
                }
            };
            let hints = vec![
                "Open authorize_url in browser".to_string(),
                format!("Poll /api/v1/connect/ac/status?run_id={}", start.run_id),
            ];
            Json(serde_json::json!(ConnectAcStartResponse {
                connection_trn: connection.trn,
                run_id: start.run_id,
                authorize_url: start.authorize_url,
                state: start.state,
                next_hints: Some(hints),
            }))
            .into_response()
        }
        ConnectMode::DeviceCode => {
            // Not implemented here; prefer CLI for now
            helpers::validation_error("unsupported", "device_code via server not yet implemented")
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ConnectAcResumeRequest {
    pub connection_trn: String,
    pub run_id: String,
    pub code: String,
    pub state: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connect/ac/resume",
    tag = "connect",
    operation_id = "connect_ac_resume",
    summary = "Resume OAuth Authorization Code flow",
    description = "Resume an OAuth2 Authorization Code flow with authorization code",
    request_body = ConnectAcResumeRequest,
    responses(
        (status = 200, description = "OAuth flow completed successfully", body = ConnectResult),
        (status = 400, description = "Invalid authorization code or state", body = crate::interface::error::ApiError),
        (status = 404, description = "OAuth flow not found or expired", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn connect_ac_resume(
    State(svc): State<OpenActService>,
    Json(req): Json<ConnectAcResumeRequest>,
) -> impl IntoResponse {
    let run_store = connect_run_store();
    // If the run_id no longer exists (e.g., timeout cleanup), return not_found
    if run_store.get(&req.run_id).is_none() {
        return helpers::not_found_error("run_id").into_response();
    }
    // Load a trivial DSL compatible with api::resume_obtain used above
    let yaml = r#"
comment: "ac resume"
startAt: "Await"
states:
  Await:
    type: task
    resource: "oauth2.await_callback"
    parameters:
      state: "{{% input.state %}}"
      expected_state: "{{% input.state %}}"
      code: "{{% input.code %}}"
    end: true
"#;
    let dsl: stepflow_dsl::WorkflowDSL = match serde_yaml::from_str(yaml) {
        Ok(d) => d,
        Err(e) => return helpers::validation_error("dsl_error", e.to_string()).into_response(),
    };
    let router = crate::authflow::actions::DefaultRouter; // not Default
    let args = crate::authflow::workflow::ResumeObtainArgs {
        run_id: req.run_id.clone(),
        code: req.code.clone(),
        state: req.state.clone(),
    };
    let out = match crate::authflow::workflow::resume_obtain(&dsl, &router, run_store, args) {
        Ok(v) => v,
        Err(e) => return helpers::execution_error(e.to_string()).into_response(),
    };
    // Try to extract auth_trn
    let auth_trn = out
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            out.get("auth_trn")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    if !auth_trn.is_empty() {
        let mgr = svc.database();
        let repo = mgr.connection_repository();
        if let Ok(Some(mut conn)) = repo.get_by_trn(&req.connection_trn).await {
            conn.auth_ref = Some(auth_trn.clone());
            let _ = repo.upsert(&conn).await;
        }
    }
    // Status & optional test
    let status = svc
        .connection_status(&req.connection_trn)
        .await
        .ok()
        .flatten();
    let test = {
        let dto = AdhocExecuteRequestDto {
            connection_trn: req.connection_trn.clone(),
            method: "GET".to_string(),
            endpoint: "https://httpbin.org/get".to_string(),
            headers: None,
            query: None,
            body: None,
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
        };
        match svc.execute_adhoc(dto).await {
            Ok(res) => Some(ExecuteResponseDto {
                status: res.status,
                headers: res.headers,
                body: res.body,
            }),
            Err(_) => None,
        }
    };
    // Record result for polling
    insert_ac_result(
        &req.run_id,
        AcResultRecord {
            done: true,
            error: None,
            auth_trn: if auth_trn.is_empty() {
                None
            } else {
                Some(auth_trn.clone())
            },
            bound_connection: Some(req.connection_trn.clone()),
            next_hints: Some(vec![
                "Check connection status".to_string(),
                "Run connection test".to_string(),
            ]),
            created_at: Some(chrono::Utc::now()),
        },
    );
    Json(serde_json::json!({ "status": status, "test": test })).into_response()
}

// ===== AC status polling =====

#[derive(Debug, Clone, Serialize, Default)]
pub(crate) struct AcResultRecord {
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_trn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_connection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_hints: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AcStatusQuery {
    pub run_id: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/connect/ac/status",
    tag = "connect",
    operation_id = "connect_ac_status",
    summary = "Check OAuth Authorization Code status",
    description = "Poll the status of an OAuth2 Authorization Code flow",
    params(
        ("run_id" = String, Query, description = "OAuth flow run ID")
    ),
    responses(
        (status = 200, description = "OAuth flow status", body = ConnectResult),
        (status = 404, description = "OAuth flow not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn connect_ac_status(Query(q): Query<AcStatusQuery>) -> impl IntoResponse {
    if let Some(mut rec) = get_ac_result(&q.run_id) {
        if rec.next_hints.is_none() {
            let hints = if !rec.done {
                vec!["Awaiting authorization in browser".to_string()]
            } else if rec.auth_trn.is_some() && rec.bound_connection.is_some() {
                vec![
                    "Check connection status".to_string(),
                    "Run connection test".to_string(),
                ]
            } else if rec.auth_trn.is_some() {
                vec!["Bind auth_trn to a connection".to_string()]
            } else {
                vec!["Retry authorization".to_string()]
            };
            rec.next_hints = Some(hints);
        }
        return Json(serde_json::json!(rec)).into_response();
    }
    let store = connect_run_store();
    let pending = store.get(&q.run_id).is_some();
    if pending {
        return Json(serde_json::json!(AcResultRecord {
            done: false,
            next_hints: Some(vec!["Awaiting authorization in browser".to_string()]),
            ..Default::default()
        }))
        .into_response();
    }
    helpers::not_found_error("run_id").into_response()
}

use crate::store::run_store::RunStore;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration as StdDuration;

pub(crate) fn connect_run_store() -> &'static crate::store::MemoryRunStore {
    static RUN_STORE: OnceLock<crate::store::MemoryRunStore> = OnceLock::new();
    RUN_STORE.get_or_init(|| crate::store::MemoryRunStore::default())
}

fn ac_results() -> &'static Arc<RwLock<HashMap<String, AcResultRecord>>> {
    static RESULTS: OnceLock<Arc<RwLock<HashMap<String, AcResultRecord>>>> = OnceLock::new();
    RESULTS.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

pub(crate) fn insert_ac_result(run_id: &str, rec: AcResultRecord) {
    let map = ac_results();
    let mut guard = map.write().unwrap();
    guard.insert(run_id.to_string(), rec);
}

pub(crate) fn get_ac_result(run_id: &str) -> Option<AcResultRecord> {
    let map = ac_results();
    let guard = map.read().unwrap();
    guard.get(run_id).cloned()
}

// Background cleaner for pending AC runs and stale results
pub(crate) fn spawn_ac_ttl_cleaner() {
    static START: OnceLock<()> = OnceLock::new();
    if START.set(()).is_err() {
        return;
    }

    // TTLs (can be tuned or moved to config): pending 10 minutes, results 30 minutes
    const RESULT_TTL_SECS: u64 = 30 * 60;
    const SWEEP_INTERVAL_SECS: u64 = 60;

    let results = ac_results().clone();
    tokio::spawn(async move {
        loop {
            // Sweep pending runs: we don't store created_at in Checkpoint; skip unless future enhancement
            // Sweep results based on created_at
            {
                let mut guard = results.write().unwrap();
                let now = chrono::Utc::now();
                let cutoff = now - chrono::Duration::seconds(RESULT_TTL_SECS as i64);
                guard.retain(|_, rec| rec.created_at.map(|t| t > cutoff).unwrap_or(true));
            }
            tokio::time::sleep(StdDuration::from_secs(SWEEP_INTERVAL_SECS)).await;
        }
    });
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodeRequest {
    pub token_url: String,
    pub device_code_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    pub tenant: String,
    pub provider: String,
    pub user_id: String,
    #[serde(default)]
    pub bind_connection: Option<String>,
    #[serde(default)]
    pub test_endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeviceCodeResponse {
    pub auth_trn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<crate::interface::dto::ConnectionStatusDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<ExecuteResponseDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_hints: Option<Vec<String>>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connect/device-code",
    tag = "connect",
    operation_id = "connect_device_code",
    summary = "Device Code OAuth flow",
    description = "Complete OAuth2 Device Code flow synchronously",
    request_body = DeviceCodeRequest,
    responses(
        (status = 200, description = "Device Code flow completed successfully", body = DeviceCodeResponse),
        (status = 400, description = "Invalid device code request or polling failed", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
/// Complete Device Code flow synchronously (issue code, poll token, persist, optional bind)
pub async fn connect_device_code(
    State(svc): State<OpenActService>,
    Json(req): Json<DeviceCodeRequest>,
) -> impl IntoResponse {
    // Step 1: device authorization request
    let mut form = vec![("client_id", req.client_id.as_str())];
    if let Some(ref s) = req.scope {
        form.push(("scope", s.as_str()));
    }
    let r = match reqwest::Client::new()
        .post(&req.device_code_url)
        .form(&form)
        .send()
        .await
    {
        Ok(x) => x,
        Err(e) => {
            return helpers::execution_error(format!("device_code request failed: {}", e))
                .with_hints(["Check device_code_url and client_id/scope", "Retry later"])
                .into_response();
        }
    };
    if !r.status().is_success() {
        return helpers::execution_error(format!("device_code request failed: {}", r.status()))
            .with_hints(["Check endpoint and credentials"])
            .into_response();
    }
    let payload: serde_json::Value = match r.json().await {
        Ok(v) => v,
        Err(e) => {
            return helpers::execution_error(e.to_string())
                .with_hints(["Endpoint returned non-JSON", "Check network connectivity"])
                .into_response();
        }
    };
    let device_code = match payload.get("device_code").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return helpers::validation_error("missing_device_code", "device_code not returned")
                .into_response();
        }
    };
    let interval = payload
        .get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);

    // Step 2: poll token endpoint
    let token_resp = loop {
        let mut form = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code.as_str()),
            ("client_id", req.client_id.as_str()),
        ];
        if let Some(ref cs) = req.client_secret {
            form.push(("client_secret", cs.as_str()));
        }
        let res = match reqwest::Client::new()
            .post(&req.token_url)
            .form(&form)
            .send()
            .await
        {
            Ok(x) => x,
            Err(e) => {
                return helpers::execution_error(e.to_string())
                    .with_hints(["Check token endpoint URL", "Verify client_id/device_code"])
                    .into_response();
            }
        };
        if res.status().is_success() {
            break res;
        }
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        if body.contains("authorization_pending") || body.contains("slow_down") {
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            continue;
        }
        return helpers::execution_error(format!("token polling failed: {} - {}", status, body))
            .with_hints(["Complete authorization in browser", "Retry if slowed down"])
            .into_response();
    };
    let token_json: serde_json::Value = match token_resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return helpers::execution_error(e.to_string())
                .with_hints(["Malformed token response"])
                .into_response();
        }
    };
    let access_token = match token_json.get("access_token").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return helpers::execution_error("missing access_token")
                .with_hints(["Ensure authorization was completed"])
                .into_response();
        }
    };
    let refresh_token = token_json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let expires_in = token_json
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    let scope_val = token_json
        .get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Step 3: persist as AuthConnection
    let ac = match crate::models::AuthConnection::new_with_params(
        req.tenant.clone(),
        req.provider.clone(),
        req.user_id.clone(),
        access_token,
        refresh_token,
        Some(expires_at),
        Some("Bearer".to_string()),
        scope_val,
        None,
    ) {
        Ok(v) => v,
        Err(e) => return helpers::execution_error(e.to_string()).into_response(),
    };
    let trn_str = ac.trn.to_string();
    if let Err(e) = svc.storage().put(&trn_str, &ac).await {
        return helpers::execution_error(e.to_string()).into_response();
    }

    // Optional bind to connection
    let mut status = None;
    let mut test = None;
    if let Some(conn_trn) = req.bind_connection.as_ref() {
        let repo = svc.database().connection_repository();
        if let Ok(Some(mut conn)) = repo.get_by_trn(conn_trn).await {
            conn.auth_ref = Some(trn_str.clone());
            let _ = repo.upsert(&conn).await;
            status = svc.connection_status(conn_trn).await.ok().flatten();
            let dto = AdhocExecuteRequestDto {
                connection_trn: conn_trn.clone(),
                method: "GET".to_string(),
                endpoint: req
                    .test_endpoint
                    .clone()
                    .unwrap_or_else(|| "https://httpbin.org/get".to_string()),
                headers: None,
                query: None,
                body: None,
                timeout_config: None,
                network_config: None,
                http_policy: None,
                response_policy: None,
                retry_policy: None,
            };
            if let Ok(res) = svc.execute_adhoc(dto).await {
                test = Some(ExecuteResponseDto {
                    status: res.status,
                    headers: res.headers,
                    body: res.body,
                });
            }
        }
    }

    let mut hints = vec!["Use auth_trn to bind a connection or proceed to requests".to_string()];
    if let Some(ref s) = status {
        if s.status != "ready" {
            hints.push("Check connection status".to_string());
        }
    }

    Json(serde_json::json!(DeviceCodeResponse {
        auth_trn: trn_str,
        status,
        test,
        next_hints: Some(hints)
    }))
    .into_response()
}
