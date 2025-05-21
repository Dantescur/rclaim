/*
  scaper/map.rs
*/

use crate::types::{AppError, BattleEvent, Location};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use std::sync::Arc;

static RECORDED_ENTRIES: Lazy<Arc<DashMap<String, ()>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static MAP_URL: &str = "https://api.chatwars.me/webview/map";

pub async fn check_for_new_entries(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<BattleEvent>, AppError> {
    tracing::debug!("Sending GET request to {}", url);
    let res = client.get(url).send().await?;
    let status = res.status();
    tracing::info!("Received response from {} with status {}", url, status);

    if status.is_client_error() || status.is_server_error() {
        tracing::error!("HTTP error: status {}", status);
        return Err(AppError::Scraper(format!("HTTP error: {}", status)));
    }

    let response = res.text().await?;
    tracing::debug!("Parsed response body ({} bytes)", response.len());

    let document = Html::parse_document(&response);
    tracing::trace!("Parsed HTML document");

    let cell_selector = Selector::parse(".map-cell")
        .map_err(|e| AppError::Scraper(format!("Failed to parse cell selector: {}", e)))?;
    let bottom_left_selector = Selector::parse(".bottom-left-text")
        .map_err(|e| AppError::Scraper(format!("Failed to parse bottom-left selector: {}", e)))?;
    let bottom_right_selector = Selector::parse(".bottom-right-text")
        .map_err(|e| AppError::Scraper(format!("Failed to parse bottom-right selector: {}", e)))?;
    let top_right_selector = Selector::parse(".top-right-text")
        .map_err(|e| AppError::Scraper(format!("Failed to parse top-right selector: {}", e)))?;
    tracing::trace!("Initialized CSS selectors");

    let mut new_events = Vec::new();

    for element in document.select(&cell_selector) {
        let bottom_left = element
            .select(&bottom_left_selector)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let bottom_right = element
            .select(&bottom_right_selector)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let top_right = element
            .select(&top_right_selector)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let location = Location::new(
            crate::auth::sanitize(&bottom_right),
            crate::auth::sanitize(&top_right),
        );

        let location_str = location.as_string();
        tracing::trace!("Processing map cell at location: {}", location_str);

        if crate::auth::sanitize(&bottom_left).contains('⚔') {
            if RECORDED_ENTRIES.insert(location_str.clone(), ()).is_none() {
                tracing::info!("New ⚔ detected at location: {}", location_str);
                new_events.push(BattleEvent { location });
            } else {
                tracing::debug!("Battle at {} already recorded", location_str);
            }
        } else if RECORDED_ENTRIES.remove(&location_str).is_some() {
            tracing::debug!("Removed expired battle at {}", location_str);
        }
    }

    tracing::info!("Found {} new battle events", new_events.len());
    Ok(new_events)
}

#[cfg(test)]
mod test {
    use super::*;
    use mockito::{Matcher, Mock, Server};
    use reqwest::Client;

    async fn setup_mock_server() -> (Mock, String) {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/webview/map")
            .match_header("accept", Matcher::Any)
            .with_status(200)
            .with_body(
                r#"
                <html>
                    <body>
                        <div class="map-cell">
                            <span class="bottom-left-text">⚔ Battle</span>
                            <span class="bottom-right-text">X1</span>
                            <span class="top-right-text">Y2</span>
                        </div>
                        <div class="map-cell">
                            <span class="bottom-left-text">Empty</span>
                            <span class="bottom-right-text">X3</span>
                            <span class="top-right-text">Y4</span>
                        </div>
                    </body>
                </html>
                "#,
            )
            .create();
        (mock, format!("{}/webview/map", server.url()))
    }

    #[tokio::test]
    async fn test_check_for_new_entries() {
        let (mock, url) = setup_mock_server().await;
        let client = Client::new();

        RECORDED_ENTRIES.clear();

        let events = check_for_new_entries(&client, &url).await.unwrap();
        assert_eq!(events.len(), 1, "Expected one battle event");
        assert_eq!(
            events[0].location.as_string(),
            "X1Y2",
            "Expected location X1Y2"
        );
        assert!(
            RECORDED_ENTRIES.contains_key("X1Y2"),
            "Expected X1Y2 in RECORDED_ENTRIES"
        );
        assert!(
            !RECORDED_ENTRIES.contains_key("X3Y4"),
            "Expected X3Y4 not in RECORDED_ENTRIES"
        );

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_check_for_new_entries_empty_response() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/webview/map")
            .match_header("accept", Matcher::Any)
            .with_status(200)
            .with_body("")
            .create();
        let client = Client::new();
        let url = format!("{}/webview/map", server.url());

        RECORDED_ENTRIES.clear();

        let events = check_for_new_entries(&client, &url).await.unwrap();
        assert_eq!(events.len(), 0, "Expected no events for empty response");
        assert!(
            RECORDED_ENTRIES.is_empty(),
            "Expected empty RECORDED_ENTRIES"
        );

        mock.assert_async().await;
    }
}
