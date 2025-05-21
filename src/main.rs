//
//  src/main.rs
//
mod auth;
mod logger;
mod scaper;
mod scheduler;
mod types;
mod ws;

use std::{env, sync::Arc};

use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use tokio::sync::broadcast;
use ws::server::WsState;

#[get("/")]
async fn health_check() -> impl Responder {
    tracing::info!("Health check requested");
    HttpResponse::Ok()
}

#[actix_web::main]
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
            p.parse::<u16>().unwrap_or_else(|e| {
                tracing::error!("Invalid PORT value: {}", e);
                panic!("PORT must be a valid number");
            })
        })
        .unwrap_or_else(|e| {
            tracing::error!("PORT not set: {}", e);
            panic!("PORT must be set");
        });

    tracing::info!("Binding server to {}:{}", host, port);

    let (event_sender, _) = broadcast::channel(100);
    tracing::debug!("Initialized broadcast channel with capacity 100");

    let client = reqwest::Client::new();
    let ws_state = web::Data::new(WsState {
        clients: Arc::new(dashmap::DashMap::new()),
        event_sender,
    });

    if let Err(e) = scheduler::start_scheduler(client, ws_state.clone()).await {
        tracing::error!("Failed to start scheduler: {}", e);
        panic!("Scheduler initialization failed");
    }
    tracing::info!("Scheduler started successfully");

    HttpServer::new(move || {
        let governor_conf = GovernorConfigBuilder::default()
            .seconds_per_request(1)
            .burst_size(100)
            .finish()
            .unwrap();
        tracing::debug!("Initialized rate limiter: 100 requests per second");
        App::new()
            .wrap(Governor::new(&governor_conf))
            .app_data(ws_state.clone())
            .service(health_check)
            .service(web::resource("/ws").route(web::get().to(ws::server::ws_handler)))
    })
    .bind((host.as_str(), port))
    .map_err(|e| {
        tracing::error!("Failed to bind server to {}:{}: {}", host, port, e);
        e
    })?
    .run()
    .await
    .map_err(|e| {
        tracing::error!("Server failed: {}", e);
        e
    })
}
