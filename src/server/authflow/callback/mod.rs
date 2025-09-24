//! OAuth2 Callback Server Module
//!
//! Provides an optional built-in HTTP server to handle OAuth2 callbacks, simplifying the integration process.
//! Requires enabling the `callback` feature.

#[cfg(feature = "callback")]
pub use callback_impl::*;

#[cfg(feature = "callback")]
mod callback_impl {
    use anyhow::{Result, anyhow};
    use axum::{
        Router,
        extract::{Query, State},
        http::StatusCode,
        response::{Html, IntoResponse},
        routing::get,
    };
    use serde::Deserialize;
    use serde_json::Value;
    use std::{
        collections::HashMap,
        net::SocketAddr,
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use stepflow_dsl::WorkflowDSL;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use crate::app::service::OpenActService;
    use crate::server::handlers::connect::{AcResultRecord, insert_ac_result};
    use crate::{
        authflow::{
            engine::TaskHandler,
            workflow::{ResumeObtainArgs, resume_obtain},
        },
        store::RunStore,
    };

    /// OAuth2 Callback Parameters
    #[derive(Debug, Clone, Deserialize)]
    pub struct CallbackParams {
        pub code: Option<String>,
        pub state: Option<String>,
        pub error: Option<String>,
        pub error_description: Option<String>,
        /// Optional: when present, server will auto-bind this connection with obtained auth
        pub connection_trn: Option<String>,
        /// Optional frontend redirect URL; server will append run_id (& connection_trn if present)
        pub redirect: Option<String>,
    }

    /// Callback Waiter - Used to wait for a callback with a specific state
    #[derive(Debug)]
    struct CallbackWaiter {
        sender: oneshot::Sender<CallbackParams>,
        created_at: Instant,
        _run_id: String,
    }

    /// Callback Server State
    #[derive(Debug, Clone)]
    struct CallbackServerState {
        waiters: Arc<Mutex<HashMap<String, CallbackWaiter>>>,
        timeout: Duration,
    }

    /// Built-in OAuth2 Callback Server
    pub struct CallbackServer {
        addr: SocketAddr,
        timeout: Duration,
        callback_path: String,
    }

    impl CallbackServer {
        /// Create a new callback server
        pub fn new(addr: impl Into<SocketAddr>) -> Self {
            Self {
                addr: addr.into(),
                timeout: Duration::from_secs(300), // 5 minutes timeout
                callback_path: "/oauth/callback".to_string(),
            }
        }

        /// Set callback timeout
        pub fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = timeout;
            self
        }

        /// Set callback path
        pub fn with_callback_path(mut self, path: impl Into<String>) -> Self {
            self.callback_path = path.into();
            self
        }

        /// Start the callback server and return a handle
        pub async fn start(self) -> Result<CallbackServerHandle> {
            let state = CallbackServerState {
                waiters: Arc::new(Mutex::new(HashMap::new())),
                timeout: self.timeout,
            };

            let app = Router::new()
                .route(&self.callback_path, get(handle_oauth_callback))
                .route("/health", get(health_check))
                .with_state(state.clone());

            let listener = tokio::net::TcpListener::bind(&self.addr).await?;
            let actual_addr = listener.local_addr()?;

            let cancel_token = CancellationToken::new();
            let server_cancel = cancel_token.clone();

            // Start the server
            let server_handle = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        server_cancel.cancelled().await;
                    })
                    .await
            });

            // Start cleanup task
            let cleanup_state = state.clone();
            let cleanup_cancel = cancel_token.clone();
            let cleanup_handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cleanup_cancel.cancelled() => {
                            break;
                        }
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {
                            let mut waiters = cleanup_state.waiters.lock().unwrap();
                            let now = Instant::now();
                            waiters.retain(|_, w| now.duration_since(w.created_at) < cleanup_state.timeout);
                        }
                    }
                }
            });

            Ok(CallbackServerHandle {
                state,
                addr: actual_addr,
                cancel_token,
                server_handle,
                cleanup_handle,
                callback_path: self.callback_path,
            })
        }
    }

    /// Callback Server Handle
    pub struct CallbackServerHandle {
        state: CallbackServerState,
        addr: SocketAddr,
        cancel_token: CancellationToken,
        server_handle: tokio::task::JoinHandle<Result<(), std::io::Error>>,
        cleanup_handle: tokio::task::JoinHandle<()>,
        callback_path: String,
    }

    impl CallbackServerHandle {
        /// Get the server listening address
        pub fn addr(&self) -> SocketAddr {
            self.addr
        }

        /// Get the full callback URL
        pub fn callback_url(&self) -> String {
            format!("http://{}{}", self.addr, self.callback_path)
        }

        /// Wait for a callback with a specific state
        pub async fn wait_for_callback(&self, state: &str, run_id: &str) -> Result<CallbackParams> {
            let (sender, receiver) = oneshot::channel();
            let waiter = CallbackWaiter {
                sender,
                created_at: Instant::now(),
                _run_id: run_id.to_string(),
            };

            // Register the waiter
            {
                let mut waiters = self.state.waiters.lock().unwrap();
                waiters.insert(state.to_string(), waiter);
            }

            // Wait for callback or timeout
            tokio::select! {
                result = receiver => {
                    result.map_err(|_| anyhow!("callback waiter cancelled"))
                }
                _ = tokio::time::sleep(self.state.timeout) => {
                    // Timeout, clean up the waiter
                    let mut waiters = self.state.waiters.lock().unwrap();
                    waiters.remove(state);
                    Err(anyhow!("callback timeout after {:?}", self.state.timeout))
                }
            }
        }

        /// Execute the full OAuth2 flow (start -> wait for callback -> resume)
        pub async fn execute_oauth_flow(
            &self,
            dsl: &WorkflowDSL,
            handler: &impl TaskHandler,
            run_store: &impl RunStore,
            mut context: Value,
        ) -> Result<Value> {
            // 1. Start the OAuth2 flow
            let start_result =
                crate::authflow::workflow::start_obtain(dsl, handler, run_store, context.take())?;

            println!("🔗 Please visit the following URL in your browser to authorize:");
            println!("   {}", start_result.authorize_url);
            println!("📡 Waiting for callback at: {}", self.callback_url());

            // 2. Wait for callback
            let callback_params = self
                .wait_for_callback(&start_result.state, &start_result.run_id)
                .await?;

            // 3. Check for errors
            if let Some(error) = callback_params.error {
                let description = callback_params
                    .error_description
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Err(anyhow!("OAuth2 error: {} - {}", error, description));
            }

            // 4. Extract authorization code
            let code = callback_params
                .code
                .ok_or_else(|| anyhow!("No authorization code received"))?;

            let state = callback_params
                .state
                .ok_or_else(|| anyhow!("No state received"))?;

            // 5. Resume the authentication process
            let run_id = start_result.run_id.clone();
            let resume_args = ResumeObtainArgs {
                run_id: run_id.clone(),
                code,
                state,
            };

            let final_result = resume_obtain(dsl, handler, run_store, resume_args)?;
            println!("✅ OAuth2 authentication completed!");

            // Record minimal result for polling (auth_trn if present)
            let auth_trn = final_result.as_str().map(|s| s.to_string()).or_else(|| {
                final_result
                    .get("auth_trn")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

            // Optional: auto-bind to a connection if provided via callback query
            let mut bound: Option<String> = None;
            // We don't have CallbackParams here; connection_trn is only available within handle_oauth_callback.
            // For auto-bind at this layer, expect connection_trn carried in the DSL context output under `bind_connection_trn` if present (optional).
            let dsl_bind_conn = final_result
                .get("bind_connection_trn")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let (Some(a), Some(conn_trn)) = (auth_trn.as_ref(), dsl_bind_conn.as_ref()) {
                if !a.is_empty() && !conn_trn.is_empty() {
                    if let Ok(svc) = OpenActService::from_env().await {
                        let repo = svc.database().connection_repository();
                        if let Ok(Some(mut conn)) = repo.get_by_trn(conn_trn).await {
                            conn.auth_ref = Some(a.clone());
                            let _ = repo.upsert(&conn).await;
                            bound = Some(conn_trn.clone());
                        }
                    }
                }
            }

            insert_ac_result(
                &run_id,
                AcResultRecord {
                    done: true,
                    error: None,
                    auth_trn,
                    bound_connection: bound,
                    next_hints: Some(vec!["Return to application and check status".to_string()]),
                    created_at: Some(chrono::Utc::now()),
                },
            );

            Ok(final_result)
        }

        /// Shutdown the server
        pub async fn shutdown(self) -> Result<()> {
            self.cancel_token.cancel();

            // Wait for server and cleanup tasks to complete
            let _ = tokio::join!(self.server_handle, self.cleanup_handle);

            Ok(())
        }
    }

    /// HTTP handler for processing OAuth2 callbacks
    async fn handle_oauth_callback(
        Query(params): Query<CallbackParams>,
        State(state): State<CallbackServerState>,
    ) -> impl IntoResponse {
        let state_param = params.state.clone().unwrap_or_default();

        // Find the corresponding waiter
        let waiter = {
            let mut waiters = state.waiters.lock().unwrap();
            waiters.remove(&state_param)
        };

        if let Some(waiter) = waiter {
            // Notify the waiting task
            let _ = waiter.sender.send(params.clone());

            // Return a success page
            let link_html = if let Some(url) = params.redirect.clone() {
                // Append run_id and optional connection_trn
                let sep = if url.contains('?') { '&' } else { '?' };
                let mut full = format!("{}{}run_id={}", url, sep, waiter._run_id);
                if let Some(ref c) = params.connection_trn {
                    full.push_str(&format!("&connection_trn={}", c));
                }
                format!("<p><a href=\"{}\">Return to application</a></p>", full)
            } else {
                String::new()
            };
            let html = format!(
                "<html><body><h1>Authentication successful</h1><p>You can close this window.</p><p>State: {}</p>{}</body></html>",
                state_param, link_html
            );
            (StatusCode::OK, Html(html)).into_response()
        } else {
            // Return an error page if no matching waiter found
            let html = "<html><body><h1>Invalid state</h1><p>No matching authentication process found.</p></body></html>";
            (StatusCode::BAD_REQUEST, Html(html.to_string())).into_response()
        }
    }

    /// Health check endpoint for the callback server
    async fn health_check() -> impl IntoResponse {
        (StatusCode::OK, Html("OK")).into_response()
    }
}
