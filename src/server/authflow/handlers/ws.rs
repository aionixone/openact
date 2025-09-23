#[cfg(feature = "server")]
use axum::extract::WebSocketUpgrade;
#[cfg(feature = "server")]
use axum::response::Response;
#[cfg(feature = "server")]
use axum::extract::State;
#[cfg(feature = "server")]
use axum::extract::ws::Message;
#[cfg(feature = "server")]
use futures::{SinkExt, StreamExt};

#[cfg(feature = "server")]
use crate::authflow::server::ServerState;

#[cfg(feature = "server")]
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
                if sender.send(Message::Text(json.into())).await.is_err() { break; }
            }
        }
    });

    let ping_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => { let _ = text; }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! { _ = broadcast_task => {}, _ = ping_task => {}, }
}


