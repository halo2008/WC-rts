//! WebSocket hub. `/ws/strategic` streams transit deltas produced by the
//! strategic tick loop; clients subscribe to the broadcast channel that lives
//! on `AppState`.

use axum::{
    Router,
    extract::{
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
};
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/strategic", get(strategic_ws))
}

async fn strategic_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_strategic(socket, state))
}

async fn handle_strategic(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            update = rx.recv() => {
                match update {
                    Ok(msg) => {
                        let json = match serde_json::to_string(&msg) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!("ws serialize failed: {}", e);
                                continue;
                            }
                        };
                        if socket
                            .send(Message::Text(Utf8Bytes::from(json)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    // Slow consumer — skip the lost messages and keep the stream alive.
                    Err(RecvError::Lagged(n)) => {
                        tracing::debug!("ws client lagged {} messages", n);
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    // Ignore pings/text/etc. — this channel is server-push only.
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}
