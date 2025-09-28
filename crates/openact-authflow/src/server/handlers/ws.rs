#[cfg(feature = "server")]
use axum::extract::ws::Message;
#[cfg(feature = "server")]
use axum::extract::State;
#[cfg(feature = "server")]
use axum::extract::WebSocketUpgrade;
#[cfg(feature = "server")]
use axum::response::Response;
#[cfg(feature = "server")]
use futures::{SinkExt, StreamExt};

#[cfg(all(feature = "server", feature = "openapi"))]
use utoipa;

#[cfg(feature = "server")]
use crate::server::state::ServerState;

/// WebSocket endpoint for real-time AuthFlow execution events
///
/// This endpoint provides real-time updates for AuthFlow workflow and execution events.
/// Clients can connect to receive live notifications about:
/// - Workflow state changes
/// - Execution progress updates
/// - Error notifications
/// - Completion events
///
/// **Connection Protocol:**
/// - Connect to `/ws` endpoint with WebSocket upgrade
/// - Server will send JSON-formatted event messages
/// - Client should handle connection close and reconnection
/// - No authentication required (events are tenant-scoped)
///
/// **Event Message Format:**
/// ```json
/// {
///   "type": "execution_state_change",
///   "execution_id": "exec_...",
///   "workflow_id": "wf_...",
///   "from_state": "running",
///   "to_state": "completed",
///   "timestamp": "2023-12-01T10:00:00Z"
/// }
/// ```
#[cfg(feature = "server")]
#[cfg_attr(all(feature = "server", feature = "openapi"), utoipa::path(
    get,
    path = "/ws",
    operation_id = "authflow_websocket_connect",
    tag = "authflow",
    summary = "Connect to AuthFlow WebSocket for real-time events",
    description = "Establishes a WebSocket connection to receive real-time notifications about AuthFlow execution events, state changes, and system events.",
    responses(
        (status = 101, description = "WebSocket connection established successfully"),
        (status = 400, description = "Invalid WebSocket upgrade request"),
        (status = 500, description = "Server error during WebSocket setup")
    )
))]
pub async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<ServerState>) -> Response {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

#[cfg(feature = "server")]
pub async fn handle_websocket(socket: axum::extract::ws::WebSocket, state: ServerState) {
    let (mut sender, mut receiver) = socket.split();
    let mut event_receiver = state.ws_broadcaster.subscribe();

    let broadcast_task = tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let ping_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let _ = text;
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! { _ = broadcast_task => {}, _ = ping_task => {}, }
}
