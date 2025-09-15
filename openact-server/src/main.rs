use authflow::engine::TaskHandler;
use axum::http::header::CONTENT_TYPE;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use manifest::storage::ExecutionRepository;
use openact_core::{
    action_registry::ActionRegistry, binding::BindingManager, database::CoreDatabase, AuthManager,
    AuthOrchestrator, ExecutionEngine,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let database_url = std::env::var("OPENACT_DATABASE_URL")
        .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
        .unwrap_or_else(|_| "sqlite:./data/openact.db".to_string());
    let db = CoreDatabase::connect(&database_url)
        .await
        .expect("db connect");
    let state = Arc::new(AppState {
        db,
        database_url: database_url.clone(),
        sessions: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/status", get(status))
        .route("/api/v1/doctor", post(doctor))
        .route("/api/v1/auth/oauth/begin", post(oauth_begin))
        .route("/api/v1/auth/oauth/complete", post(oauth_complete))
        .route("/api/v1/oauth/callback", get(oauth_callback))
        .route("/api/v1/auth/pat", post(auth_pat))
        .route("/api/v1/auth", get(list_auth))
        .route("/api/v1/auth/:trn", get(get_auth).delete(delete_auth))
        .route("/api/v1/auth/:trn/refresh", post(auth_refresh))
        .route("/api/v1/actions", post(register_action).get(list_actions))
        .route(
            "/api/v1/actions/:trn",
            get(inspect_action).put(update_action).delete(delete_action),
        )
        .route("/api/v1/actions/:trn/export", get(export_action))
        .route(
            "/api/v1/bindings",
            post(bind_binding).get(list_bindings).delete(delete_binding),
        )
        .route("/api/v1/run", post(run_action))
        .route("/api/v1/executions/:exec_trn", get(get_execution))
        .route(
            "/api/v1/sessions",
            get(list_sessions).delete(clean_sessions),
        )
        .route(
            "/api/v1/sessions/:id",
            get(get_session).delete(delete_session),
        )
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr = std::env::var("OPENACT_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:8088".to_string());
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[allow(dead_code)]
async fn not_implemented() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": {"code": "NotImplemented", "message": "Endpoint not implemented"}
        })),
    )
}

struct AppState {
    db: CoreDatabase,
    database_url: String,
    sessions: Mutex<HashMap<String, serde_json::Value>>,
}

#[derive(Serialize, ToSchema)]
struct HealthResp {
    ok: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses((status = 200, description = "OK", body = HealthResp))
)]
async fn health() -> Json<HealthResp> {
    Json(HealthResp { ok: true })
}

#[derive(Serialize, ToSchema)]
struct StatusResp {
    bindings: u64,
    actions: u64,
    auth_connections: u64,
    encryption: serde_json::Value,
}

#[utoipa::path(
    get,
    path = "/api/v1/status",
    responses((status = 200, description = "OK", body = StatusResp))
)]
async fn status(State(state): State<Arc<AppState>>) -> (StatusCode, Json<StatusResp>) {
    let stats = state
        .db
        .stats()
        .await
        .unwrap_or(openact_core::database::CoreStats {
            bindings: 0,
            actions: 0,
            auth_connections: 0,
        });
    let master_from = if std::env::var("AUTHFLOW_MASTER_KEY").is_ok() {
        Some("AUTHFLOW_MASTER_KEY")
    } else if std::env::var("OPENACT_MASTER_KEY").is_ok() {
        Some("OPENACT_MASTER_KEY")
    } else {
        None
    };
    let enc = serde_json::json!({
        "master_key": master_from.unwrap_or("(not set)"),
        "key_version_env": std::env::var("AUTHFLOW_KEY_VERSION").ok()
    });
    (
        StatusCode::OK,
        Json(StatusResp {
            bindings: stats.bindings,
            actions: stats.actions,
            auth_connections: stats.auth_connections,
            encryption: enc,
        }),
    )
}

#[derive(Deserialize, ToSchema)]
struct DoctorReq {
    #[serde(default)]
    dsl: Option<String>,
    #[serde(default = "default_port_start", rename = "portStart")]
    port_start: u16,
    #[serde(default = "default_port_end", rename = "portEnd")]
    port_end: u16,
}
fn default_port_start() -> u16 {
    8080
}
fn default_port_end() -> u16 {
    8099
}

#[derive(Serialize, ToSchema)]
struct DoctorResp {
    db_url: String,
    master_key_set: bool,
    db_connectivity: bool,
    free_ports_sample: Vec<u16>,
    dsl_check: Option<String>,
    suggestions: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/doctor",
    request_body = DoctorReq,
    responses((status = 200, description = "OK", body = DoctorResp))
)]
async fn doctor(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DoctorReq>,
) -> (StatusCode, Json<DoctorResp>) {
    // DB URL and master key
    let db_url = std::env::var("OPENACT_DATABASE_URL")
        .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
        .unwrap_or_else(|_| state.database_url.clone());
    let master_set =
        std::env::var("OPENACT_MASTER_KEY").is_ok() || std::env::var("AUTHFLOW_MASTER_KEY").is_ok();
    let mut suggestions: Vec<String> = Vec::new();
    if db_url.is_empty() {
        if let Ok(pwd) = std::env::current_dir() {
            suggestions.push(format!(
                "export OPENACT_DATABASE_URL=sqlite:{}/manifest/data/openact.db",
                pwd.display()
            ));
        }
    }
    if !master_set {
        suggestions.push("export OPENACT_MASTER_KEY=your-32-bytes-key".to_string());
    }

    // DB connectivity
    let db_ok = state.db.health_check().await.is_ok();
    if !db_ok {
        suggestions.push("Check database URL and file permissions".to_string());
    }

    // Ports sample
    let mut free_ports = Vec::new();
    for p in req.port_start..=req.port_end {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() {
            free_ports.push(p);
            if free_ports.len() >= 3 {
                break;
            }
        }
    }
    if free_ports.is_empty() {
        suggestions.push(
            "Choose different --portStart/--portEnd or pass --redirect to auth endpoints"
                .to_string(),
        );
    }

    // DSL secrets check
    let mut dsl_check: Option<String> = None;
    if let Some(path) = req.dsl {
        let orch = AuthOrchestrator::new(state.db.pool().clone());
        let res = orch
            .begin_oauth_from_config(
                "doctor",
                std::path::Path::new(&path),
                Some("OAuth"),
                Some("http://localhost:8080/oauth/callback"),
                Some("user:email"),
            )
            .await;
        match res {
            Ok((url, _)) => {
                dsl_check = Some(format!("OK: {}", url));
            }
            Err(e) => {
                dsl_check = Some(format!("{}", e));
                if !e.to_string().contains("authorize_url") {
                    suggestions.push(
                        "Ensure OPENACT_SECRETS_FILE or environment secrets are set".to_string(),
                    );
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(DoctorResp {
            db_url,
            master_key_set: master_set,
            db_connectivity: db_ok,
            free_ports_sample: free_ports,
            dsl_check,
            suggestions,
        }),
    )
}

#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct ErrorBody {
    code: String,
    message: String,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct ErrorResp {
    error: ErrorBody,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct DeleteResp {
    deleted: bool,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct UnboundResp {
    unbound: bool,
}

// ===== OAuth Begin/Complete =====

#[derive(Deserialize, ToSchema)]
struct OauthBeginReq {
    tenant: String,
    dsl: String,
    #[serde(default)]
    flow: Option<String>,
    #[serde(default, rename = "redirectUri")]
    redirect_uri: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct OauthBeginResp {
    authorize_url: String,
    session_id: String,
    redirect_uri: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/oauth/begin",
    request_body = OauthBeginReq,
    responses((status = 200, description = "OK", body = OauthBeginResp))
)]
async fn oauth_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OauthBeginReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let orch = AuthOrchestrator::new(state.db.pool().clone());
    let path = std::path::Path::new(&req.dsl);
    let (auth_url, pending) = match orch
        .begin_oauth_from_config(
            &req.tenant,
            path,
            req.flow.as_deref(),
            req.redirect_uri.as_deref(),
            req.scope.as_deref(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}}),
                ),
            );
        }
    };
    let sid = Uuid::new_v4().to_string();
    let mut sess = state.sessions.lock().await;
    // try extract state from authorize_url ("...state=xxxxx&...")
    let state_q = auth_url.split('?').nth(1).and_then(|q| {
        q.split('&')
            .find(|kv| kv.starts_with("state="))
            .map(|kv| kv.trim_start_matches("state=").to_string())
    });
    let sess_obj = serde_json::json!({
        "tenant": req.tenant,
        "dsl": req.dsl,
        "redirect_uri": req.redirect_uri,
        "flow_name": pending.flow_name,
        "next_state": pending.next_state,
        "context": pending.context,
        "state": state_q,
        "session_id": sid,
        "authorize_url": auth_url,
    });
    sess.insert(sid.clone(), sess_obj.clone());
    if let Some(st) = state_q {
        sess.insert(format!("state:{}", st), sess_obj);
    }
    drop(sess);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "authorize_url": auth_url,
            "session_id": sid,
            "redirect_uri": req.redirect_uri
        })),
    )
}

#[utoipa::path(
    get,
    path = "/api/v1/oauth/callback",
    params(
        ("code" = String, Query, description = "OAuth authorization code"),
        ("state" = String, Query, description = "Opaque state to correlate session"),
    ),
    responses((status = 200, description = "OK"))
)]
async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let code_opt = params.get("code").cloned();
    let state_opt = params.get("state").cloned();
    // find session by state if provided, else fail
    if state_opt.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": "missing state"}})),
        );
    }
    let state_key = format!("state:{}", state_opt.clone().unwrap());
    let mut sessions = state.sessions.lock().await;
    let Some(v) = sessions.get(&state_key).cloned() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BadRequest", "message": "session not found for state"}}),
            ),
        );
    };
    // also remove the strong session id if exists
    if let Some(sid) = v.get("session_id").and_then(|x| x.as_str()) {
        sessions.remove(sid);
    }
    sessions.remove(&state_key);
    drop(sessions);

    let _tenant = v
        .get("tenant")
        .and_then(|x| x.as_str())
        .unwrap_or("default")
        .to_string();
    let dsl_path = v
        .get("dsl")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let flow = v
        .get("flow_name")
        .and_then(|x| x.as_str())
        .unwrap_or("OAuth")
        .to_string();
    let next_state = v
        .get("next_state")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let mut context = v.get("context").cloned().unwrap_or(serde_json::json!({}));

    // build callback url from current request params (we only need code/state)
    if let Some(code) = code_opt.clone() {
        if let serde_json::Value::Object(ref mut o) = context {
            o.insert("code".into(), serde_json::Value::String(code));
        }
    }
    if let Some(st) = state_opt.clone() {
        if let serde_json::Value::Object(ref mut o) = context {
            o.insert("state".into(), serde_json::Value::String(st));
        }
    }

    let orch = AuthOrchestrator::new(state.db.pool().clone());
    let path = std::path::Path::new(&dsl_path);
    match orch
        .resume_with_context(path, &flow, &next_state, context)
        .await
    {
        Ok(trn) => (StatusCode::OK, Json(serde_json::json!({"auth_trn": trn}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}})),
        ),
    }
}

#[derive(Deserialize, ToSchema)]
struct OauthCompleteReq {
    tenant: String,
    dsl: String,
    session_id: String,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct OauthCompleteResp {
    auth_trn: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/oauth/complete",
    request_body = OauthCompleteReq,
    responses((status = 200, description = "OK", body = OauthCompleteResp))
)]
async fn oauth_complete(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OauthCompleteReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let orch = AuthOrchestrator::new(state.db.pool().clone());
    let path = std::path::Path::new(&req.dsl);
    // load session
    let mut sessions = state.sessions.lock().await;
    let Some(v) = sessions.remove(&req.session_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BadRequest", "message": "invalid session"}}),
            ),
        );
    };
    let flow = v
        .get("flow_name")
        .and_then(|x| x.as_str())
        .unwrap_or("OAuth")
        .to_string();
    let next_state = v
        .get("next_state")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let mut context = v.get("context").cloned().unwrap_or(serde_json::json!({}));
    drop(sessions);

    // inject code/state if provided directly
    if let Some(code) = req.code.clone() {
        if let serde_json::Value::Object(ref mut o) = context {
            o.insert("code".into(), serde_json::Value::String(code));
        }
    }
    if let Some(state_s) = req.state.clone() {
        if let serde_json::Value::Object(ref mut o) = context {
            o.insert("state".into(), serde_json::Value::String(state_s));
        }
    }

    // If callback_url provided, let core parse internally via complete_oauth_with_callback using pending; fallback to resume_with_context
    if let Some(cb) = req.callback_url.clone() {
        // Reconstruct pending
        let (_auth_url, pend) = match orch
            .begin_oauth_from_config(&req.tenant, path, Some(&flow), None, None)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}}),
                    ),
                );
            }
        };
        let mut pending = pend;
        // overwrite with saved state
        pending.next_state = next_state.clone();
        pending.context = context.clone();
        match orch.complete_oauth_with_callback(pending, &cb) {
            Ok(trn) => return (StatusCode::OK, Json(serde_json::json!({"auth_trn": trn}))),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}}),
                    ),
                )
            }
        }
    }

    // resume with context path
    match orch
        .resume_with_context(path, &flow, &next_state, context)
        .await
    {
        Ok(trn) => (StatusCode::OK, Json(serde_json::json!({"auth_trn": trn}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}})),
        ),
    }
}

// ===== Auth Refresh =====
#[derive(Deserialize, ToSchema)]
struct AuthRefreshReq {
    /// OAuth token endpoint URL (provider-specific)
    token_url: String,
    /// OAuth client ID (provider-specific)
    client_id: String,
    /// OAuth client secret (provider-specific)
    client_secret: String,
}

#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct AuthRefreshResp {
    auth_trn: String,
    refreshed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/{trn}/refresh",
    params(("trn" = String, Path, description = "Auth TRN")),
    request_body = AuthRefreshReq,
    responses((status = 200, description = "OK", body = AuthRefreshResp))
)]
async fn auth_refresh(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
    Json(req): Json<AuthRefreshReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    match AuthManager::from_database_url(state.database_url.clone()).await {
        Ok(am) => {
            // Get current connection
            let conn = match am.get(&trn).await {
                Ok(Some(c)) => c,
                Ok(None) => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(
                            serde_json::json!({"error": {"code": "NotFound", "message": "auth connection not found"}}),
                        ),
                    )
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}}),
                        ),
                    )
                }
            };

            // Check if refresh_token exists
            let refresh_token = match &conn.refresh_token {
                Some(rt) => rt,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            serde_json::json!({"error": {"code": "BadRequest", "message": "no refresh token available"}}),
                        ),
                    )
                }
            };

            // All refresh parameters are required (provider-agnostic)
            let token_url = &req.token_url;
            let client_id = &req.client_id;
            let client_secret = &req.client_secret;

            // Create refresh request using AuthFlow's OAuth2RefreshTokenHandler
            let handler = authflow::actions::OAuth2RefreshTokenHandler;
            let refresh_req = serde_json::json!({
                "tokenUrl": token_url,
                "clientId": client_id,
                "clientSecret": client_secret,
                "refresh_token": refresh_token
            });

            match handler.execute("oauth2.refresh_token", "refresh", &refresh_req) {
                Ok(result) => {
                    // Extract new tokens
                    let access_token = result
                        .get("access_token")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let new_refresh_token = result.get("refresh_token").and_then(|v| v.as_str());
                    let expires_in = result.get("expires_in").and_then(|v| v.as_i64());

                    // Update connection
                    match am
                        .refresh_connection(&trn, access_token, new_refresh_token, expires_in)
                        .await
                    {
                        Ok(_) => (
                            StatusCode::OK,
                            Json(serde_json::json!({"auth_trn": trn, "refreshed": true})),
                        ),
                        Err(e) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                serde_json::json!({"error": {"code": "Internal", "message": format!("failed to update connection: {}", e)}}),
                            ),
                        ),
                    }
                }
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": {"code": "BadRequest", "message": format!("token refresh failed: {}", e)}}),
                    ),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

// ===== PAT Create =====
#[derive(Deserialize, ToSchema)]
struct PatReq {
    tenant: String,
    provider: String,
    user_id: String,
    token: String,
}
#[allow(dead_code)]
#[derive(Serialize, ToSchema)]
struct PatResp {
    auth_trn: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/pat",
    request_body = PatReq,
    responses((status = 200, description = "OK", body = PatResp))
)]
async fn auth_pat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    match AuthManager::from_database_url(state.database_url.clone()).await {
        Ok(am) => match am
            .create_pat_connection(&req.tenant, &req.provider, &req.user_id, &req.token)
            .await
        {
            Ok(trn) => (StatusCode::OK, Json(serde_json::json!({"auth_trn": trn}))),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}}),
                ),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

// ===== Sessions Management =====
#[utoipa::path(get, path = "/api/v1/sessions", responses((status = 200, description = "OK")))]
async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sessions = state.sessions.lock().await;
    let session_list: Vec<serde_json::Value> = sessions
        .iter()
        .map(|(id, data)| {
            serde_json::json!({
                "session_id": id,
                "tenant": data.get("tenant").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "flow_name": data.get("flow_name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "next_state": data.get("next_state").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "authorize_url": data.get("authorize_url").and_then(|v| v.as_str()).unwrap_or(""),
                "state": data.get("state").and_then(|v| v.as_str()).unwrap_or("")
            })
        })
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({"sessions": session_list, "count": session_list.len()})),
    )
}

#[utoipa::path(delete, path = "/api/v1/sessions", responses((status = 200, description = "OK")))]
async fn clean_sessions(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut sessions = state.sessions.lock().await;
    let count = sessions.len();
    sessions.clear();
    (StatusCode::OK, Json(serde_json::json!({"cleaned": count})))
}

#[utoipa::path(get, path = "/api/v1/sessions/{id}", params(("id" = String, Path, description = "Session ID")), responses((status = 200, description = "OK")))]
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sessions = state.sessions.lock().await;
    match sessions.get(&id) {
        Some(data) => (StatusCode::OK, Json(data.clone())),
        None => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"error": {"code": "NotFound", "message": "session not found"}}),
            ),
        ),
    }
}

#[utoipa::path(delete, path = "/api/v1/sessions/{id}", params(("id" = String, Path, description = "Session ID")), responses((status = 200, description = "OK")))]
async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut sessions = state.sessions.lock().await;
    match sessions.remove(&id) {
        Some(_) => (StatusCode::OK, Json(serde_json::json!({"deleted": true}))),
        None => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"error": {"code": "NotFound", "message": "session not found"}}),
            ),
        ),
    }
}

// ===== Run & Executions =====
#[derive(Deserialize, ToSchema)]
struct RunReq {
    tenant: String,
    action_trn: String,
    #[serde(default)]
    exec_trn: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    trace: Option<bool>,
    #[serde(default)]
    input_data: Option<serde_json::Value>,
    #[serde(default)]
    pagination: Option<PaginationReq>,
}

#[derive(Deserialize, ToSchema, Default)]
struct PaginationReq {
    #[serde(default)]
    all_pages: bool,
    #[serde(default)]
    max_pages: Option<u64>,
    #[serde(default)]
    per_page: Option<u64>,
}

#[utoipa::path(post, path = "/api/v1/run", request_body = RunReq, responses((status = 200, description = "OK")))]
async fn run_action(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = ExecutionEngine::new(state.db.clone());
    let exec_trn = req.exec_trn.unwrap_or_else(|| {
        format!(
            "trn:exec:{}:{}:{}",
            req.tenant,
            "api",
            chrono::Utc::now().timestamp_millis()
        )
    });

    if req.dry_run.unwrap_or(false) {
        let preview = serde_json::json!({
            "tenant": req.tenant,
            "action_trn": req.action_trn,
            "exec_trn": exec_trn,
            "output": req.output.clone().unwrap_or_else(|| "json".to_string()),
            "trace": req.trace.unwrap_or(false),
            "input_data": req.input_data,
        });
        return (
            StatusCode::OK,
            Json(serde_json::json!({"preview": preview})),
        );
    }

    let mut input_opt: Option<openact_core::ActionInput> = req.input_data.and_then(|v| {
        let path_params = v
            .get("path_params")
            .and_then(|x| x.as_object())
            .map(|m| m.clone());
        let query = v
            .get("query")
            .and_then(|x| x.as_object())
            .map(|m| m.clone());
        let headers = v.get("headers").and_then(|x| x.as_object()).map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        });
        let body = v.get("body").cloned();
        Some(openact_core::ActionInput {
            path_params: path_params.map(|m| m.into_iter().map(|(k, v)| (k, v)).collect()),
            query: query.map(|m| m.into_iter().map(|(k, v)| (k, v)).collect()),
            headers,
            body,
            pagination: None,
        })
    });

    if let Some(p) = req.pagination {
        let po = openact_core::PaginationOptions {
            all_pages: p.all_pages,
            max_pages: p.max_pages,
            per_page: p.per_page,
        };
        input_opt = Some(match input_opt {
            Some(mut i) => {
                i.pagination = Some(po);
                i
            }
            None => openact_core::ActionInput {
                path_params: None,
                query: None,
                headers: None,
                body: None,
                pagination: Some(po),
            },
        });
    }

    match engine
        .run_action_by_trn_with_input(&req.tenant, &req.action_trn, &exec_trn, input_opt)
        .await
    {
        Ok(res) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "data": {
                    "status": res.status,
                    "response": res.response_data,
                    "error": res.error_message,
                    "status_code": res.status_code,
                    "duration_ms": res.duration_ms,
                    "exec_trn": exec_trn,
                    "action_trn": req.action_trn,
                }
            })),
        ),
        Err(e) => {
            let code = StatusCode::BAD_REQUEST;
            (
                code,
                Json(serde_json::json!({
                    "ok": false,
                    "error": {"code": "BadRequest", "message": e.to_string()},
                    "meta": {"exec_trn": exec_trn, "action_trn": req.action_trn, "tenant": req.tenant}
                })),
            )
        }
    }
}

#[utoipa::path(get, path = "/api/v1/executions/{exec_trn}", responses((status = 200, description = "OK")))]
async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(exec_trn): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let erepo = ExecutionRepository::new(state.db.pool().clone());
    let _ = erepo.ensure_table_exists().await; // ensure table exists before querying
    match erepo.get_execution_by_trn(&exec_trn).await {
        Ok(row) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "data": {
                    "execution_trn": row.execution_trn,
                    "action_trn": row.action_trn,
                    "tenant": row.tenant,
                    "status": row.status,
                    "status_code": row.status_code,
                    "error_message": row.error_message,
                    "duration_ms": row.duration_ms,
                    "output_data": row.output_data,
                    "created_at": row.created_at,
                    "completed_at": row.completed_at,
                }
            })),
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"ok": false, "error": {"code": "NotFound", "message": "execution not found"}}),
            ),
        ),
    }
}
#[utoipa::path(
    delete,
    path = "/api/v1/actions/{trn}",
    params(("trn" = String, Path, description = "Action TRN")),
    responses(
        (status = 200, description = "Deleted", body = DeleteResp),
        (status = 500, description = "Error", body = ErrorResp)
    )
)]
async fn delete_action(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ActionRegistry::new(state.db.pool().clone());
    match registry.delete_by_trn(&trn).await {
        Ok(ok) => (StatusCode::OK, Json(serde_json::json!({"deleted": ok}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/auth/{trn}",
    params(("trn" = String, Path, description = "Auth TRN")),
    responses(
        (status = 200, description = "Deleted", body = DeleteResp),
        (status = 500, description = "Error", body = ErrorResp)
    )
)]
async fn delete_auth(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match AuthManager::from_database_url(state.database_url.clone()).await {
        Ok(am) => match am.delete(&trn).await {
            Ok(ok) => (StatusCode::OK, Json(serde_json::json!({"deleted": ok}))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[derive(Deserialize, ToSchema)]
struct UnbindReq {
    tenant: String,
    auth_trn: String,
    action_trn: String,
}

#[utoipa::path(
    delete,
    path = "/api/v1/bindings",
    request_body = UnbindReq,
    responses(
        (status = 200, description = "Unbound", body = UnboundResp),
        (status = 500, description = "Error", body = ErrorResp)
    )
)]
async fn delete_binding(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnbindReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let bm = BindingManager::new(state.db.pool().clone());
    match bm.unbind(&req.tenant, &req.auth_trn, &req.action_trn).await {
        Ok(ok) => (StatusCode::OK, Json(serde_json::json!({"unbound": ok}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[derive(Deserialize, ToSchema)]
struct BindReq {
    tenant: String,
    auth_trn: String,
    action_trn: String,
}

#[utoipa::path(post, path = "/api/v1/bindings", request_body = BindReq, responses((status = 200, description = "OK")))]
async fn bind_binding(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BindReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let bm = BindingManager::new(state.db.pool().clone());
    match bm
        .bind(&req.tenant, &req.auth_trn, &req.action_trn, Some("api"))
        .await
    {
        Ok(b) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"tenant": b.tenant, "auth_trn": b.auth_trn, "action_trn": b.action_trn}),
            ),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}})),
        ),
    }
}

#[derive(Deserialize, ToSchema)]
struct ListBindingsQuery {
    #[serde(default)]
    tenant: Option<String>,
    #[serde(default)]
    auth_trn: Option<String>,
    #[serde(default)]
    action_trn: Option<String>,
    #[serde(default)]
    verbose: Option<bool>,
}

#[utoipa::path(get, path = "/api/v1/bindings", responses((status = 200, description = "OK")))]
async fn list_bindings(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListBindingsQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tenant = q.tenant.clone().unwrap_or_else(|| "default".to_string());
    let bm = BindingManager::new(state.db.pool().clone());
    match bm.list_by_tenant(&tenant).await {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows.into_iter().filter(|b| {
                q.auth_trn.as_ref().map(|v| &b.auth_trn == v).unwrap_or(true) && q.action_trn.as_ref().map(|v| &b.action_trn == v).unwrap_or(true)
            }).map(|b| {
                if q.verbose.unwrap_or(false) {
                    serde_json::json!({"tenant": b.tenant, "auth_trn": b.auth_trn, "action_trn": b.action_trn, "created_by": b.created_by, "created_at": b.created_at})
                } else {
                    serde_json::json!({"auth_trn": b.auth_trn, "action_trn": b.action_trn})
                }
            }).collect();
            (StatusCode::OK, Json(serde_json::json!({"items": items})))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        status,
        doctor,
        oauth_begin,
        oauth_complete,
        auth_refresh,
        auth_pat,
        list_auth,
        get_auth,
        list_actions,
        inspect_action,
        register_action,
        update_action,
        export_action,
        run_action,
        get_execution,
        bind_binding,
        list_bindings,
        delete_action,
        delete_auth,
        delete_binding,
        list_sessions,
        clean_sessions,
        get_session,
        delete_session,
    ),
    tags(
        (name = "OpenAct API", description = "HTTP API v1 - Draft"),
    )
)]
struct ApiDoc;

// ===== Auth list/inspect =====
#[derive(Deserialize, ToSchema)]
struct AuthListQuery {
    #[serde(default)]
    _tenant: Option<String>,
    #[serde(default)]
    _provider: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/auth", responses((status = 200, description = "OK")))]
async fn list_auth(
    State(state): State<Arc<AppState>>,
    Query(_q): Query<AuthListQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    match AuthManager::from_database_url(state.database_url.clone()).await {
        Ok(am) => match am.list().await {
            Ok(v) => (StatusCode::OK, Json(serde_json::json!({"items": v}))),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[utoipa::path(get, path = "/api/v1/auth/{trn}", responses((status = 200, description = "OK")))]
async fn get_auth(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match AuthManager::from_database_url(state.database_url.clone()).await {
        Ok(am) => match am.get(&trn).await {
            Ok(Some(conn)) => (
                StatusCode::OK,
                Json(
                    serde_json::json!({"trn": conn.trn.to_trn_string().unwrap_or_default(), "token_type": conn.token_type, "scope": conn.scope, "extra": conn.extra}),
                ),
            ),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(
                    serde_json::json!({"error": {"code": "NotFound", "message": "auth not found"}}),
                ),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

// ===== Actions list/inspect/register/update/export =====
#[derive(Deserialize, ToSchema)]
struct RegisterActionReq {
    tenant: String,
    provider: String,
    name: String,
    trn: String,
    yaml: String,
}

#[utoipa::path(get, path = "/api/v1/actions", responses((status = 200, description = "OK")))]
async fn list_actions(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tenant = q
        .get("tenant")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let registry = ActionRegistry::new(state.db.pool().clone());
    match registry.list_by_tenant(&tenant).await {
        Ok(items) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"items": items.into_iter().map(|a| { serde_json::json!({"trn": a.trn, "tenant": a.tenant, "provider": a.provider, "name": a.name, "is_active": a.is_active}) }).collect::<Vec<_>>() }),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "Internal", "message": e.to_string()}})),
        ),
    }
}

#[utoipa::path(get, path = "/api/v1/actions/{trn}", responses((status = 200, description = "OK")))]
async fn inspect_action(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ActionRegistry::new(state.db.pool().clone());
    match registry.get_by_trn(&trn).await {
        Ok(a) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"trn": a.trn, "tenant": a.tenant, "provider": a.provider, "name": a.name, "is_active": a.is_active}),
            ),
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": {"code": "NotFound", "message": "action not found"}})),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/actions",
    request_body = RegisterActionReq,
    responses((status = 200, description = "OK"))
)]
async fn register_action(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterActionReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ActionRegistry::new(state.db.pool().clone());
    // write yaml to temp file to reuse registry API
    let mut f = NamedTempFile::new().unwrap();
    let _ = f.write_all(req.yaml.as_bytes());
    match registry
        .register_from_yaml(&req.tenant, &req.provider, &req.name, &req.trn, f.path())
        .await
    {
        Ok(a) => (StatusCode::OK, Json(serde_json::json!({"trn": a.trn}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}})),
        ),
    }
}

#[derive(Deserialize, ToSchema)]
struct UpdateActionReq {
    yaml: String,
}

#[utoipa::path(
    put,
    path = "/api/v1/actions/{trn}",
    request_body = UpdateActionReq,
    responses((status = 200, description = "OK"))
)]
async fn update_action(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
    Json(req): Json<UpdateActionReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let registry = ActionRegistry::new(state.db.pool().clone());
    let mut f = NamedTempFile::new().unwrap();
    let _ = f.write_all(req.yaml.as_bytes());
    match registry.update_from_yaml(&trn, f.path()).await {
        Ok(a) => (StatusCode::OK, Json(serde_json::json!({"trn": a.trn}))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "BadRequest", "message": e.to_string()}})),
        ),
    }
}

#[utoipa::path(get, path = "/api/v1/actions/{trn}/export", responses((status = 200, description = "OK")))]
async fn export_action(
    State(state): State<Arc<AppState>>,
    Path(trn): Path<String>,
) -> impl IntoResponse {
    let registry = ActionRegistry::new(state.db.pool().clone());
    match registry.export_spec_by_trn(&trn).await {
        Ok(spec) => (StatusCode::OK, [(CONTENT_TYPE, "text/yaml")], spec).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": {"code": "NotFound", "message": "action not found"}})),
        )
            .into_response(),
    }
}
