//
//  src/scheduler.rs
//

use std::env;
use std::sync::Arc;

use crate::scaper::map::{MAP_URL, check_for_new_entries};
use crate::types::AppError;
use crate::ws::server::{WsState, broadcast_events};
use reqwest::Client;

pub async fn start_scheduler(client: Client, ws_state: Arc<WsState>) -> Result<(), AppError> {
    tracing::debug!("Starting scheduler task");
    let client = client.clone();
    let ws_state = Arc::clone(&ws_state);

    tokio::spawn(async move {
        loop {
            tracing::info!("Checking for new entries...");
            match check_for_new_entries(&client, MAP_URL).await {
                Ok(events) if !events.is_empty() => {
                    tracing::debug!("Broadcasting {} events", events.len());
                    broadcast_events(ws_state.clone(), &events).await;
                }
                Ok(_) => {
                    tracing::debug!("No new events found")
                }
                Err(e) => tracing::error!("Error checking entries: {}", e),
            }
            let interval = env::var("SCHEDULE_INTERVAL")
                .map(|s| s.parse::<u64>().unwrap_or(60))
                .unwrap_or(60);
            tracing::trace!("Sleeping for {} seconds", interval);
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        }
    });

    Ok(())
}
