//! Axum HTTP + WebSocket server for the GNSS web UI.
//!
//! # Usage
//!
//! ```no_run
//! use tokio::sync::broadcast;
//! use gnss_server::web_server::{run_web_server, AppState};
//!
//! let (ws_tx, _) = broadcast::channel::<String>(64);
//! // tokio::spawn(run_web_server(8080, ws_tx));
//! ```
//!
//! The static HTML page is embedded at compile time via `include_str!`.
//! All WebSocket clients receive broadcast messages from `ws_tx`.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use tokio::sync::broadcast;

const INDEX_HTML: &str = include_str!("../static/index.html");

/// Shared state for the web server — holds the broadcast sender for WebSocket fan-out.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub ws_tx: broadcast::Sender<String>,
}

/// Start the HTTP + WebSocket server on the given port.
///
/// Returns when the server exits (or on error). Call via `tokio::spawn`.
#[allow(dead_code)]
pub async fn run_web_server(port: u16, ws_tx: broadcast::Sender<String>) -> anyhow::Result<()> {
    let state = AppState { ws_tx };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("Web server listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve the embedded HTML page.
async fn index_handler() -> impl IntoResponse {
    Html(INDEX_HTML)
}

/// Upgrade HTTP to WebSocket and hand off to handle_socket.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.ws_tx.subscribe()))
}

/// Drive a single WebSocket connection — forward broadcast messages to the client.
async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    loop {
        tokio::select! {
            recv_result = rx.recv() => {
                match recv_result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() {
                            // Client disconnected or send failed
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Slow client — skip missed messages and continue
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                if msg.is_none() {
                    // Client closed the connection
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_html_not_empty() {
        assert!(!INDEX_HTML.is_empty());
    }
}
