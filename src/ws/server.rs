/*
* src/ws/server.rs
*/

use crate::types::{AppError, BattleEvent};
use crate::ws::client::{Client, ClientMap, is_rate_limited};
use actix_web::{HttpRequest, HttpResponse, web};
use actix_ws::{Message, MessageStream, Session};
use chrono::Utc;
use futures_util::stream::StreamExt;
use scopeguard::defer;
use tokio::sync::broadcast;

pub struct WsState {
    pub clients: ClientMap,
    pub event_sender: broadcast::Sender<BattleEvent>,
}

pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<WsState>,
) -> Result<HttpResponse, actix_web::Error> {
    let token = req
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|h| h.to_str().ok());
    tracing::debug!("WebSocket connection attempt with token: {:?}", token);

    crate::auth::is_valid_client(token).map_err(|e| {
        tracing::warn!("Unauthorized WebSocket connection: {}", e);
        actix_web::error::ErrorUnauthorized(e)
    })?;

    let (response, session, stream) = actix_ws::handle(&req, stream)?;
    let client_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("New WebSocket client connected: {}", client_id);

    state.clients.insert(
        client_id.clone(),
        Client {
            request_count: 1,
            window_start: Some(Utc::now()),
        },
    );

    actix_web::rt::spawn(async move {
        defer!({
            tracing::info!("Cleaning up client {}", client_id);
            state.clients.remove(&client_id);
        });
        if let Err(e) = handle_client(session, stream, &state, &client_id).await {
            tracing::error!("Client error: {}", e);
        }
    });

    Ok(response)
}

async fn handle_client(
    mut session: Session,
    mut stream: MessageStream,
    state: &web::Data<WsState>,
    client_id: &str,
) -> Result<(), AppError> {
    tracing::debug!("Sending welcome message to client {}", client_id);
    session
        .text("Connected to the notification service!")
        .await
        .map_err(|e| {
            tracing::error!("Failed to send welcome message: {}", e);
            AppError::WebSocket(e)
        })?;

    let mut event_receiver = state.event_sender.subscribe();
    tracing::debug!("Client {} subscribed to event channel", client_id);

    loop {
        tokio::select! {
                            Some(msg) = stream.next() => {
                                match msg {
                                    Ok(Message::Text(text)) => {
                                    tracing::info!("Client {} sent message: {}", client_id, text);
                                        if let Some(mut client) = state.clients.get_mut(client_id) {
                                            if is_rate_limited(&mut client) {
                                            tracing::warn!("Client {} rate limit exceeded", client_id);
                                                session
                                                    .text("Rate limit exceeded. Try again later.")
                                                    .await?;
                                            return Err(AppError::RateLimitExceeded);
                                            }
                                        }
                                    }
                                    Ok(Message::Close(reason)) => {
        tracing::info!("Client {} disconnected: {:?}", client_id, reason);
                                        break;
                                    }
                                    Ok(msg) => {
                     tracing::debug!("Client {} received unhandled message: {:?}", client_id, msg);
                }
                                    Err(e) => {
                                        tracing::error!("Error receiving message for client {}: {}", client_id, e);
                                        break;
                                    }
                                }
                            }
                            Ok(event) = event_receiver.recv() => {
                                let msg = format!("New ⚔ detected at location: {}", event.location.as_string());
                tracing::debug!("Sending event to client {}: {}", client_id, msg);
                                if let Err(e) = session.text(msg.as_str()).await {
                                    tracing::error!("Failed to send to client {}: {}", client_id, e);
                                    break;
                                }
                            }
                        }
    }

    tracing::info!("Client {} cleanup completed", client_id);
    // state.clients.remove(client_id);
    Ok(())
}

pub async fn broadcast_events(state: &web::Data<WsState>, events: &[BattleEvent]) {
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
