//
//  src/main.rs
//
mod auth;
mod logger;
mod scaper;
mod scheduler;
mod types;
mod ws;

use std::{env, net::SocketAddr, sync::Arc};

use axum::{Router, response::IntoResponse, routing::get};
use reqwest::StatusCode;
use tokio::sync::broadcast;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use ws::server::WsState;

async fn health_check() -> impl IntoResponse {
    tracing::info!("Health Check requested");
    StatusCode::OK
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    logger::init_logger();
    tracing::info!("Starting rclaim server...");

    dotenvy::dotenv().ok();

    let host = env::var("HOST").unwrap_or_else(|_| {
        tracing::warn!("HOST not set, defaulting to 127.0.0.1");
        "127.0.0.1".to_string()
    });

    let port = env::var("PORT")
        .map(|p| {
            p.parse::<u16>().map_err(|e| {
                tracing::error!("Invalid PORT value: {}", e);
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "PORT must be a valid number",
                )
            })
        })
        .unwrap_or_else(|e| {
            tracing::error!("PORT not set: {}", e);
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "PORT must be set",
            ))
        })?;

    let addr: SocketAddr = format!("{}:{}", host, port).parse().map_err(|e| {
        tracing::error!("Failed to parse address: {}:{} {}", host, port, e);
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
    })?;

    tracing::info!("Binding server to {}", addr);

    let (event_sender, _) = broadcast::channel(100);
    tracing::debug!("Initialized broadcast channel with capacity 100");

    let client = reqwest::Client::new();
    let ws_state = Arc::new(WsState {
        clients: Arc::new(dashmap::DashMap::new()),
        event_sender,
    });

    scheduler::start_scheduler(client, ws_state.clone())
        .await
        .map_err(|e| {
            tracing::error!("Failed to start scheduler: {}", e);
            std::io::Error::other(e.to_string())
        })?;

    tracing::info!("Scheduler started successfully");

    let governor_conf = GovernorConfigBuilder::default()
        .per_second(1)
        .burst_size(100)
        .use_headers()
        .key_extractor(GlobalKeyExtractor)
        .finish()
        .unwrap();

    tracing::debug!("Initialized rate limiter: 100 requests per second");

    let app = Router::new()
        .route("/", get(health_check))
        .route("/ws", get(ws::server::ws_handler))
        .layer(GovernorLayer {
            config: Arc::new(governor_conf),
        })
        .with_state(ws_state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
    Ok(())
}
