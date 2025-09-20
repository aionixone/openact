//! OAuth2 Callback Server Module
//! 
//! Provides an optional built-in HTTP server to handle OAuth2 callbacks, simplifying the integration process.
//! Requires enabling the `callback` feature.

#[cfg(feature = "callback")]
pub use callback_impl::*;

#[cfg(feature = "callback")]
mod callback_impl {
use anyhow::{anyhow, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
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

use crate::{
    api::{resume_obtain, ResumeObtainArgs},
    authflow::engine::TaskHandler,
    store::RunStore,
};

/// OAuth2 Callback Parameters
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// Callback Waiter - Used to wait for a callback with a specific state
#[derive(Debug)]
struct CallbackWaiter {
    sender: oneshot::Sender<CallbackParams>,
    created_at: Instant,
    run_id: String,
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
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        cleanup_expired_waiters(&cleanup_state);
                    }
                    _ = cleanup_cancel.cancelled() => {
                        break;
                    }
                }
            }
        });

        Ok(CallbackServerHandle {
            addr: actual_addr,
            state,
            cancel_token,
            server_handle,
            cleanup_handle,
            callback_path: self.callback_path,
        })
    }
}

/// Callback Server Handle
pub struct CallbackServerHandle {
    addr: SocketAddr,
    state: CallbackServerState,
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
            run_id: run_id.to_string(),
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
        let start_result = crate::api::start_obtain(dsl, handler, run_store, context.take())?;

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
        let resume_args = ResumeObtainArgs {
            run_id: start_result.run_id,
            code,
            state,
        };

        let final_result = resume_obtain(dsl, handler, run_store, resume_args)?;
        println!("✅ OAuth2 authentication completed!");

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
        // Send callback parameters to the waiter
        let _ = waiter.sender.send(params);

        (
            StatusCode::OK,
            Html(r#"
            <html>
            <head><title>Authentication Successful</title></head>
            <body>
                <h1>🎉 Authentication Successful!</h1>
                <p>You can close this page and return to the application.</p>
                <script>
                    // Attempt to close the window (if it's a popup)
                    setTimeout(() => {
                        window.close();
                    }, 2000);
                </script>
            </body>
            </html>
            "#),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            Html(r#"
            <html>
            <head><title>Authentication Error</title></head>
            <body>
                <h1>❌ Authentication Error</h1>
                <p>Invalid callback request or request has expired.</p>
            </body>
            </html>
            "#),
        )
    }
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    "OK"
}

/// Clean up expired waiters
fn cleanup_expired_waiters(state: &CallbackServerState) {
    let mut waiters = state.waiters.lock().unwrap();
    let now = Instant::now();

    // Collect expired keys first
    let expired_keys: Vec<String> = waiters
        .iter()
        .filter(|(_, waiter)| now.duration_since(waiter.created_at) > state.timeout)
        .map(|(k, _)| k.clone())
        .collect();

    // Remove expired waiters and send timeout error
    for key in expired_keys {
        if let Some(waiter) = waiters.remove(&key) {
            let _ = waiter.sender.send(CallbackParams {
                code: None,
                state: None,
                error: Some("timeout".to_string()),
                error_description: Some("Callback timeout".to_string()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_callback_server_startup() {
        let server = CallbackServer::new("127.0.0.1:0".parse::<SocketAddr>().unwrap());
        let handle = server.start().await.unwrap();

        assert!(handle.addr().port() > 0);
        assert!(handle.callback_url().contains("/oauth/callback"));

        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_callback_timeout() {
        let server = CallbackServer::new("127.0.0.1:0".parse::<SocketAddr>().unwrap())
            .with_timeout(Duration::from_millis(100));
        let handle = server.start().await.unwrap();

        let result = handle.wait_for_callback("test_state", "test_run").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));

        handle.shutdown().await.unwrap();
    }
}

} // end of callback_impl module
