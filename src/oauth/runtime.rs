use crate::utils::trn::{make_auth_cc_token_trn, parse_connection_trn};
use crate::store::connection_store::ConnectionStore; // bring trait into scope
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::OnceCell;
use tokio::sync::{Mutex, oneshot};
use tracing::debug;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub scope: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefreshOutcome {
    Fresh(TokenInfo),
    Reused(TokenInfo),
    Refreshed(TokenInfo),
}

/// 统一 OAuth 运行时接口（Phase 0：仅接口，占位实现）
pub trait AuthRuntime: Send + Sync {
    fn name(&self) -> &'static str {
        "OpenActAuthRuntime"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    /// 获取或刷新 Client Credentials 令牌（带 singleflight 语义，后续实现）
    fn get_cc_token<'a>(
        &'a self,
        connection_trn: &'a str,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<RefreshOutcome>> + Send + 'a>>;

    /// 根据需要刷新 Authorization Code 的访问令牌
    fn refresh_ac_if_needed<'a>(
        &'a self,
        connection_trn: &'a str,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<RefreshOutcome>> + Send + 'a>>;
}

/// 默认占位实现（未实现任何逻辑）
struct OpenActAuthRuntime;

impl AuthRuntime for OpenActAuthRuntime {
    fn get_cc_token<'a>(
        &'a self,
        _connection_trn: &'a str,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<RefreshOutcome>> + Send + 'a>>
    {
        Box::pin(async move { get_cc_token_impl(_connection_trn).await })
    }

    fn refresh_ac_if_needed<'a>(
        &'a self,
        _connection_trn: &'a str,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<RefreshOutcome>> + Send + 'a>>
    {
        Box::pin(async move { refresh_ac_impl(_connection_trn, None).await })
    }
}

static AUTH_RUNTIME: OnceCell<Arc<dyn AuthRuntime>> = OnceCell::const_new();
static AUTH_STORE: OnceLock<Arc<dyn crate::store::ConnectionStore>> = OnceLock::new();

pub fn set_auth_store(store: Arc<dyn crate::store::ConnectionStore>) -> bool {
    AUTH_STORE.set(store).is_ok()
}

#[cfg(test)]
pub fn reset_auth_store() {
    // We can't directly reset OnceLock, but we can force create_connection_store path
    // by using OPENACT_FORCE_ENV_STORE=1 env var
}

/// 设置全局 OAuth 运行时（仅能设置一次）
pub fn set_auth_runtime(runtime: Arc<dyn AuthRuntime>) -> bool {
    AUTH_RUNTIME.set(runtime).is_ok()
}

/// 获取全局 OAuth 运行时（若未设置，则使用默认占位实现）
pub fn auth_runtime() -> Arc<dyn AuthRuntime> {
    if let Some(r) = AUTH_RUNTIME.get() {
        return r.clone();
    }
    let default: Arc<dyn AuthRuntime> = Arc::new(OpenActAuthRuntime);
    let _ = AUTH_RUNTIME.set(default.clone());
    default
}

#[allow(dead_code)]
pub async fn get_cc_token(connection_trn: &str) -> Result<RefreshOutcome> {
    auth_runtime().get_cc_token(connection_trn).await
}

#[allow(dead_code)]
pub async fn refresh_ac_if_needed(connection_trn: &str) -> Result<RefreshOutcome> {
    // Backward-compatible helper uses no explicit auth_ref
    refresh_ac_for(connection_trn, None).await
}

#[allow(dead_code)]
pub async fn refresh_ac_for(
    connection_trn: &str,
    auth_ref: Option<&str>,
) -> Result<RefreshOutcome> {
    refresh_ac_impl(connection_trn, auth_ref).await
}

// ===== Implementation for default runtime =====

static CC_TOKEN_CACHE: OnceLock<Mutex<HashMap<String, (String, DateTime<Utc>)>>> = OnceLock::new();
static CC_INFLIGHT: OnceLock<
    Mutex<HashMap<String, Vec<oneshot::Sender<anyhow::Result<(String, DateTime<Utc>)>>>>>,
> = OnceLock::new();

async fn register_cc_inflight(
    trn: &str,
) -> Result<Option<oneshot::Receiver<anyhow::Result<(String, DateTime<Utc>)>>>> {
    let map = CC_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().await;
    if let Some(waiters) = guard.get_mut(trn) {
        let (tx, rx) = oneshot::channel();
        waiters.push(tx);
        return Ok(Some(rx));
    } else {
        guard.insert(trn.to_string(), Vec::new());
        return Ok(None);
    }
}

async fn finish_cc_inflight(trn: &str, val: anyhow::Result<(String, DateTime<Utc>)>) -> Result<()> {
    let map = CC_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().await;
    if let Some(waiters) = guard.remove(trn) {
        for tx in waiters {
            let _ = tx.send(
                val.as_ref()
                    .map(|(t, e)| (t.clone(), *e))
                    .map_err(|e| anyhow!("{}", e)),
            );
        }
    }
    Ok(())
}

async fn get_cached_cc_token(trn: &str, skew_seconds: i64) -> Option<String> {
    let cache = CC_TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = cache.lock().await;
    if let Some((token, exp)) = guard.get(trn) {
        if *exp > Utc::now() + Duration::seconds(skew_seconds) {
            return Some(token.clone());
        }
    }
    None
}

async fn cache_cc_token(trn: &str, token: String, expires_at: DateTime<Utc>) {
    let cache = CC_TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().await;
    guard.insert(trn.to_string(), (token, expires_at));
}

async fn maybe_get_db_cc_token(trn: &str) -> Result<Option<(String, DateTime<Utc>)>> {
    // Use unified StorageService as the token store (single path)
    let service = crate::store::service::StorageService::global().await;
    let (tenant, conn_id) = parse_connection_trn(trn)?;
    let token_trn = make_auth_cc_token_trn(&tenant, &conn_id);
    if let Some(conn) = service.get(&token_trn).await? {
        if let Some(exp) = conn.expires_at {
            if exp > Utc::now() + Duration::seconds(60) {
                return Ok(Some((conn.access_token, exp)));
            }
        }
    }
    Ok(None)
}

async fn get_cc_token_impl(connection_trn: &str) -> Result<RefreshOutcome> {
    // 1) DB first
    if let Some((token, exp)) = maybe_get_db_cc_token(connection_trn).await? {
        cache_cc_token(connection_trn, token.clone(), exp).await;
        debug!(target: "oauth_runtime", cc_db_hit=true, trn=%connection_trn, "reuse cc token from DB");
        return Ok(RefreshOutcome::Reused(TokenInfo {
            access_token: token,
            refresh_token: None,
            expires_at: Some(exp),
            scope: None,
        }));
    }

    // 2) Memory cache
    if let Some(token) = get_cached_cc_token(connection_trn, 60).await {
        debug!(target: "oauth_runtime", cc_mem_hit=true, trn=%connection_trn, "reuse cc token from memory");
        return Ok(RefreshOutcome::Reused(TokenInfo {
            access_token: token,
            refresh_token: None,
            expires_at: None,
            scope: None,
        }));
    }

    // 3) Singleflight wait
    if let Some(rx) = register_cc_inflight(connection_trn).await? {
        let (token, exp) = rx.await.map_err(|_| anyhow!("inflight channel closed"))??;
        debug!(target: "oauth_runtime", cc_inflight_join=true, trn=%connection_trn, "joined inflight cc token");
        return Ok(RefreshOutcome::Reused(TokenInfo {
            access_token: token,
            refresh_token: None,
            expires_at: Some(exp),
            scope: None,
        }));
    }

    // 4) Fetch connection & request new token
    use std::time::Duration as StdDuration;
    let service = crate::store::service::StorageService::global().await;
    let connection = service
        .get_connection_cached(connection_trn, StdDuration::from_secs(30))
        .await?
        .ok_or_else(|| anyhow!("Connection not found: {}", connection_trn))?;
    tracing::info!(target: "oauth_runtime", trn=%connection_trn, "loaded connection for refresh");

    let oauth_params = connection
        .auth_parameters
        .oauth_parameters
        .as_ref()
        .ok_or_else(|| {
            anyhow!(
                "OAuth2 parameters missing for connection: {}",
                connection.trn
            )
        })?;

    let mut request_params = json!({
        "tokenUrl": oauth_params.token_url,
        "clientId": oauth_params.client_id,
        "clientSecret": oauth_params.client_secret,
    });
    if let Some(scope) = &oauth_params.scope {
        request_params["scopes"] = json!(scope.split_whitespace().collect::<Vec<_>>());
    }

    use crate::authflow::actions::OAuth2ClientCredentialsHandler;
    use crate::authflow::engine::TaskHandler;
    let handler = OAuth2ClientCredentialsHandler::default();
    let result = handler
        .execute("oauth2.client_credentials", "", &request_params)
        .context("Failed to obtain OAuth2 Client Credentials token")?;

    let access_token = result
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No access_token in OAuth2 response"))?;

    let now = Utc::now();
    let expires_in = result
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    let expires_at = now + Duration::seconds(expires_in);

    // Persist to DB via unified StorageService
    use crate::models::AuthConnection;
    let service = crate::store::service::StorageService::global().await;
    let ac = AuthConnection::new_with_params(
        "openact",
        "oauth2_cc",
        connection_trn,
        access_token.to_string(),
        None,
        Some(expires_at),
        Some("Bearer".to_string()),
        oauth_params.scope.clone(),
        None,
    )
    .map_err(|e| anyhow!("failed to create AuthConnection: {}", e))?;
    let trn_str = ac.trn.to_string();
    if service.put(&trn_str, &ac).await.is_ok() {
        let _ = service.cleanup_expired().await;
        debug!(target: "oauth_runtime", cc_db_persist=true, trn=%connection_trn, "persisted cc token to DB");
    }

    cache_cc_token(connection_trn, access_token.to_string(), expires_at).await;
    let _ = finish_cc_inflight(connection_trn, Ok((access_token.to_string(), expires_at))).await;

    Ok(RefreshOutcome::Fresh(TokenInfo {
        access_token: access_token.to_string(),
        refresh_token: None,
        expires_at: Some(expires_at),
        scope: oauth_params.scope.clone(),
    }))
}

async fn refresh_ac_impl(connection_trn: &str, auth_ref: Option<&str>) -> Result<RefreshOutcome> {
    // Load connection for OAuth client parameters
    use std::time::Duration as StdDuration;
    let service = crate::store::service::StorageService::global().await;
    let connection = service
        .get_connection_cached(connection_trn, StdDuration::from_secs(30))
        .await?
        .ok_or_else(|| anyhow!("Connection not found: {}", connection_trn))?;

    let oauth_params = connection
        .auth_parameters
        .oauth_parameters
        .as_ref()
        .ok_or_else(|| {
            anyhow!(
                "OAuth2 parameters missing for connection: {}",
                connection.trn
            )
        })?;

    // Use the same StorageService instance as the auth token store to ensure single database instance
    let store: Arc<dyn crate::store::ConnectionStore> = service.clone();

    // Try direct refresh via DB refresh_token first
    // Choose standardized auth connection TRN:
    // - If explicit auth_ref provided, use it as full TRN
    // - Else derive from connection.auth_ref if present (already a full TRN per spec)
    // - Else error (AC requires explicit reference)
    let ac_trn = if let Some(r) = auth_ref {
        r.to_string()
    } else if let Some(r) = connection.auth_ref.as_deref() {
        r.to_string()
    } else {
        return Err(anyhow!(
            "OAuth2 AC requires auth_ref TRN on connection or parameter"
        ));
    };
    let fetched = store.get(&ac_trn).await?;
    tracing::info!(target: "oauth_runtime", trn=%connection_trn, auth_trn=%ac_trn, has_record=%fetched.is_some(), "fetch auth connection from store");
    if let Some(existing) = fetched {
        // If token is still fresh (skew 60s), reuse directly
        if let Some(exp) = existing.expires_at {
            if exp > Utc::now() + Duration::seconds(60) {
                tracing::info!(target: "oauth_runtime", ac_reuse_db=true, trn=%connection_trn, auth_trn=%ac_trn, "reuse fresh ac token from DB");
                return Ok(RefreshOutcome::Reused(TokenInfo {
                    access_token: existing.access_token.clone(),
                    refresh_token: existing.refresh_token.clone(),
                    expires_at: existing.expires_at,
                    scope: existing.scope.clone(),
                }));
            }
        }

        if let Some(rt) = existing.refresh_token.clone() {
            tracing::info!(target: "oauth_runtime", ac_refresh_direct=true, trn=%connection_trn, auth_trn=%ac_trn, "attempt direct refresh with stored refresh_token");
            // Build form request to token endpoint
            let form = [
                ("grant_type", "refresh_token"),
                ("refresh_token", rt.as_str()),
                ("client_id", oauth_params.client_id.as_str()),
                ("client_secret", oauth_params.client_secret.as_str()),
            ];
            let resp = reqwest::Client::new()
                .post(&oauth_params.token_url)
                .form(&form)
                .send()
                .await
                .context("refresh_token request failed")?;
            if resp.status().is_success() {
                let json: serde_json::Value =
                    resp.json().await.context("invalid token response json")?;
                let access_token = json
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("no access_token in refresh response"))?
                    .to_string();
                let new_refresh_token = json
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or(existing.refresh_token.clone());
                let expires_in = json
                    .get("expires_in")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(3600);
                let expires_at = Utc::now() + Duration::seconds(expires_in);

                // persist back - reuse the original TRN parts to maintain consistency
                let mut updated = existing.clone();
                updated.access_token = access_token.clone();
                updated.refresh_token = new_refresh_token.clone();
                updated.expires_at = Some(expires_at);
                updated.updated_at = Utc::now();
                let trn_auth = updated.trn.to_string();
                let _ = store.put(&trn_auth, &updated).await;
                tracing::info!(target: "oauth_runtime", ac_refresh_direct_ok=true, trn=%connection_trn, auth_trn=%ac_trn, "refreshed ac token via refresh_token");
                return Ok(RefreshOutcome::Refreshed(TokenInfo {
                    access_token,
                    refresh_token: new_refresh_token,
                    expires_at: Some(expires_at),
                    scope: existing.scope.clone(),
                }));
            } else {
                tracing::info!(target: "oauth_runtime", ac_refresh_direct_fail=true, trn=%connection_trn, auth_trn=%ac_trn, status=%resp.status().as_u16(), "refresh_token request failed; will fallback");
            }
        }
    }

    // Fallback: Ensure fresh token via authflow handler
    use crate::authflow::actions::EnsureFreshTokenHandler;
    use crate::authflow::engine::TaskHandler;

    let ensure_handler = EnsureFreshTokenHandler { store };
    let request_params = json!({
        "connection_ref": ac_trn,
        "tokenUrl": oauth_params.token_url,
        "clientId": oauth_params.client_id,
        "clientSecret": oauth_params.client_secret,
        "skewSeconds": 120
    });

    let result = ensure_handler
        .execute("ensure.fresh_token", "", &request_params)
        .context("Failed to ensure fresh OAuth2 token")?;

    let access_token = result
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!(
            "OAuth2 Authorization Code token not found or expired. Please re-authorize connection: {}",
            connection.trn
        ))?;

    debug!(target: "oauth_runtime", ac_ensure=true, trn=%connection_trn, "ensured fresh ac token via handler");
    Ok(RefreshOutcome::Refreshed(TokenInfo {
        access_token: access_token.to_string(),
        refresh_token: None,
        expires_at: None,
        scope: oauth_params.scope.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthorizationType, ConnectionConfig, OAuth2Parameters};
    use crate::store::service::StorageService;
    use chrono::Utc;
    use httpmock::prelude::*;
    use tempfile::tempdir;

    fn make_cc_connection(trn: &str, name: &str, token_url: String) -> ConnectionConfig {
        let mut c = ConnectionConfig::new(
            trn.to_string(),
            name.to_string(),
            AuthorizationType::OAuth2ClientCredentials,
        );
        c.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            token_url,
            scope: Some("r:all".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        c
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn test_cc_token_db_persistence_and_reuse() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("openact.db");
        unsafe {
            std::env::set_var(
                "OPENACT_DB_URL",
                format!("sqlite:{}?mode=rwc", db_path.display()),
            );
        }
        // unified backend; no separate auth backend flag

        // Mock token endpoint
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(serde_json::json!({
                    "access_token": "T1",
                    "expires_in": 3600
                }));
        });

        // Init storage and insert connection1
        let service = StorageService::from_env().await.unwrap();
        let service = Arc::new(service);
        crate::store::service::set_global_storage_service_for_tests(service.clone()).await;
        let conn1 = make_cc_connection(
            "trn:openact:default:connection/cc1",
            "cc1",
            format!("{}{}", server.base_url(), "/token"),
        );
        service.upsert_connection(&conn1).await.unwrap();

        // First acquire via network, persist to DB
        let out1 = get_cc_token(&conn1.trn).await.unwrap();
        let token1 = match out1 {
            RefreshOutcome::Fresh(info)
            | RefreshOutcome::Reused(info)
            | RefreshOutcome::Refreshed(info) => info.access_token,
        };
        assert_eq!(token1, "T1");

        // Prepare connection2 and pre-populate DB
        let conn2 = make_cc_connection(
            "trn:openact:default:connection/cc2",
            "cc2",
            format!("{}{}", server.base_url(), "/token"),
        );
        service.upsert_connection(&conn2).await.unwrap();

        use crate::models::AuthConnection;
        let service = StorageService::global().await;
        let expires_at = Utc::now() + chrono::Duration::seconds(1800);
        let ac = AuthConnection::new_with_params(
            "openact",
            "oauth2_cc",
            &conn2.trn,
            "DBTOKEN".to_string(),
            None,
            Some(expires_at),
            Some("Bearer".to_string()),
            Some("r:all".to_string()),
            None,
        )
        .unwrap();
        let trn_auth = ac.trn.to_string();
        service.put(&trn_auth, &ac).await.unwrap();

        // Now fetch via runtime — should reuse DB token immediately (no network)
        let out2 = get_cc_token(&conn2.trn).await.unwrap();
        let token2 = match out2 {
            RefreshOutcome::Fresh(info)
            | RefreshOutcome::Reused(info)
            | RefreshOutcome::Refreshed(info) => info.access_token,
        };
        assert_eq!(token2, "DBTOKEN");
    }

    fn make_ac_connection(trn: &str, name: &str) -> ConnectionConfig {
        // token_url not used in test, EnsureFreshToken reads from store
        let mut c = ConnectionConfig::new(
            trn.to_string(),
            name.to_string(),
            AuthorizationType::OAuth2AuthorizationCode,
        );
        c.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            token_url: "https://example.com/token".to_string(),
            scope: Some("r:all".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        c
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn test_ac_refresh_ensure_fresh_token_path() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("openact.db");
        unsafe {
            std::env::set_var(
                "OPENACT_DB_URL",
                format!("sqlite:{}?mode=rwc", db_path.display()),
            );
        }
        unsafe {
            std::env::set_var("OPENACT_AUTH_BACKEND", "sqlite");
        }

        let service = StorageService::global().await;
        let conn = make_ac_connection("trn:openact:default:connection/ac1", "ac1");
        service.upsert_connection(&conn).await.unwrap();

        // Seed store with an existing access token for AC flow
        use crate::models::AuthConnection;
        let service = StorageService::global().await;
        let expires_at = Utc::now() + chrono::Duration::seconds(600);
        let ac = AuthConnection::new_with_params(
            "openact",
            "oauth2_ac",
            &conn.trn,
            "ACTOKEN".to_string(),
            Some("RFTOKEN".to_string()),
            Some(expires_at),
            Some("Bearer".to_string()),
            Some("r:all".to_string()),
            None,
        )
        .unwrap();
        let trn_auth = ac.trn.to_string();
        service.put(&trn_auth, &ac).await.unwrap();

        // runtime should return the token via EnsureFreshToken path
        let out = refresh_ac_if_needed(&conn.trn).await.unwrap();
        let token = match out {
            RefreshOutcome::Fresh(info)
            | RefreshOutcome::Reused(info)
            | RefreshOutcome::Refreshed(info) => info.access_token,
        };
        assert_eq!(token, "ACTOKEN");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ac_direct_refresh_with_auth_ref() {
        // Reset global state for test isolation
        crate::store::service::reset_global_storage_for_tests().await;
        
        // Setup DB env
        let dir = tempdir().unwrap();
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let db_path = dir.path().join(format!("test_ac_direct_{}.db", ts));
        unsafe {
            std::env::set_var(
                "OPENACT_DB_URL",
                format!("sqlite:{}?mode=rwc", db_path.display()),
            );
        }
        unsafe {
            std::env::set_var("OPENACT_AUTH_BACKEND", "sqlite");
        }

        // Mock token endpoint for refresh
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(serde_json::json!({
                    "access_token": "NEWAC",
                    "refresh_token": "NEWRF",
                    "expires_in": 1800
                }));
        });

        // Insert connection config (needed to read client credentials and token_url)
        let svc = StorageService::from_env().await.unwrap();
        let service = Arc::new(svc);
        crate::store::service::set_global_storage_service_for_tests(service.clone()).await;
        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/ac-direct".to_string(),
            "ac-direct".to_string(),
            AuthorizationType::OAuth2AuthorizationCode,
        );
        conn.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "cid".to_string(),
            client_secret: "secret".to_string(),
            token_url: format!("{}{}", server.base_url(), "/token"),
            scope: Some("read".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        service.upsert_connection(&conn).await.unwrap();

        // Seed auth connection with refresh token under an auth_ref TRN (use unified service)
        let auth_ref = "trn:openact:default:auth/oauth2_ac-alice";
        let ac = crate::models::AuthConnection::new_with_params(
            "openact",
            "oauth2_ac",
            "alice",
            "OLDAC".to_string(),
            Some("RFTOKEN".to_string()),
            // Make the existing token expired so refresh path is taken
            Some(Utc::now() - chrono::Duration::seconds(10)),
            Some("Bearer".to_string()),
            Some("read".to_string()),
            None,
        )
        .unwrap();
        service.put(auth_ref, &ac).await.unwrap();
        assert!(service.get(auth_ref).await.unwrap().is_some());

        // **KEY FIX**: Inject the same storage service instance for OAuth runtime
        crate::store::service::set_global_storage_service_for_tests(service.clone()).await;
        
        // Call runtime direct refresh using auth_ref
        let out = super::refresh_ac_for(&conn.trn, Some(auth_ref))
            .await
            .unwrap();
        // Expect refreshed (we set refresh token and token response always returns new)
        match out {
            RefreshOutcome::Refreshed(info) => {
                assert_eq!(info.access_token, "NEWAC");
                assert_eq!(info.refresh_token.as_deref(), Some("NEWRF"));
            }
            other => panic!("unexpected outcome: {:?}", other),
        }
    }
}
