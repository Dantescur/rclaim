//
//  src/scheduler.rs
//

use crate::scaper::map::{MAP_URL, check_for_new_entries};
use crate::types::AppError;
use crate::ws::server::broadcast_events;
use reqwest::Client;

pub async fn start_scheduler(
    client: Client,
    ws_state: actix_web::web::Data<crate::ws::server::WsState>,
) -> Result<(), AppError> {
    tracing::debug!("Starting scheduler task");
    let client = client.clone();
    let ws_state = ws_state.clone();

    tokio::spawn(async move {
        loop {
            tracing::info!("Checking for new entries...");
            match check_for_new_entries(&client, MAP_URL).await {
                Ok(events) if !events.is_empty() => {
                    tracing::debug!("Broadcasting {} events", events.len());
                    broadcast_events(&ws_state, &events).await;
                }
                Ok(_) => {
                    tracing::debug!("No new events found")
                }
                Err(e) => tracing::error!("Error checking entries: {}", e),
            }
            tracing::trace!("Sleeping for 60 seconds");
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    Ok(())
}
