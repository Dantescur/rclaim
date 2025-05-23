/*
* src/ws/server.rs
*/

use std::sync::Arc;

use crate::types::{AppError, BattleEvent};
use crate::ws::client::{Client, ClientMap, is_rate_limited};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use chrono::Utc;
use tokio::sync::broadcast;

pub struct WsState {
    pub clients: ClientMap,
    pub event_sender: broadcast::Sender<BattleEvent>,
}

struct ClientGuard {
    clients: ClientMap,
    client_id: String,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        tracing::info!("Cleaning up client {}", self.client_id);
        self.clients.remove(&self.client_id);
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<WsState>>,
) -> impl IntoResponse {
    let protocol_header = headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok());

    let maybe_token = protocol_header.and_then(|s| s.strip_prefix("token-"));
    tracing::debug!("WebSocket connection attempt with token: {:?}", maybe_token);

    if maybe_token.is_none() {
        tracing::warn!("Missing or Invalid Sec-WebSocket-Protocol header");
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }

    let token = maybe_token.unwrap();

    if let Err(err) = crate::auth::is_valid_client(Some(token)) {
        tracing::warn!("Invalid token: {}", err);
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }

    let client_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("New WebSocket client connected: {}", client_id);

    state.clients.insert(
        client_id.clone(),
        Client {
            request_count: 1,
            window_start: Some(Utc::now()),
        },
    );

    ws.protocols(["token-auth"])
        .on_upgrade(move |socket| async move {
            let guard = ClientGuard {
                clients: state.clients.clone(),
                client_id: client_id.clone(),
            };
            if let Err(e) = handle_client(socket, state, client_id.clone()).await {
                tracing::error!("WebSocket error: {}", e);
            }
            drop(guard);
        })
}

async fn handle_client(
    mut socket: WebSocket,
    state: Arc<WsState>,
    client_id: String,
) -> Result<(), AppError> {
    tracing::debug!("Sending welcome message to client {}", client_id);

    if let Err(e) = socket
        .send(Message::Text(
            "Connected to the notification service!".into(),
        ))
        .await
    {
        return Err(AppError::WebSocket(e));
    }

    let mut event_receiver = state.event_sender.subscribe();
    tracing::debug!("Client {} subscribed to event channel", client_id);

    loop {
        tokio::select! {
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        tracing::info!("Client {} sent message: {}", client_id, text);
                        if let Some(mut client) = state.clients.get_mut(&client_id) {
                            if is_rate_limited(&mut client) {
                                tracing::warn!("Client {} rate limit exceeded", client_id);
                                socket.send(Message::Text("Rate limit exceeded. Try again later.".into())).await.ok();
                                return Err(AppError::RateLimitExceeded);
                            }
                        }
                    },
                    Ok(Message::Close(reason)) => {
                        tracing::info!("Client {} disconnected: {:?}", client_id, reason);
                        break;
                    }
                    Ok(_) => {} // ping/pong etc
                    Err(e) => {
                        tracing::error!("WebSocket receive error for client {}: {}", client_id, e);
                        break;
                    }
                }
            }
            Ok(event) = event_receiver.recv() => {
                let msg = format!("New ⚔ detected at location: {}", event.location.as_string());
                tracing::debug!("Sending event to client {}: {}", client_id, msg);
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    tracing::error!("Failed to send event to client {}", client_id);
                    break;
                }
            }
        }
    }

    tracing::info!("Client {} cleanup completed", client_id);
    Ok(())
}

pub async fn broadcast_events(state: Arc<WsState>, events: &[BattleEvent]) {
    tracing::debug!("Broadcasting {} events", events.len());
    if state.event_sender.receiver_count() == 0 {
        tracing::debug!("No subscribers for broadcast channel, skipping send event.");
        return;
    }
    for event in events {
        tracing::trace!("Sending event: {:?}", event);
        if let Err(e) = state.event_sender.send(event.clone()) {
            tracing::error!("Failed to send event to channel: {}", e);
        }
    }
}
