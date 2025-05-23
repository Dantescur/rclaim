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

static CELL_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".map-cell").expect("Failed to parse cell selector at compile time")
});
static BOTTOM_LEFT_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".bottom-left-text")
        .expect("Failed to parse bottom-left selector at compile time")
});
static BOTTOM_RIGHT_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".bottom-right-text")
        .expect("Failed to parse bottom-right selector at compile time")
});
static TOP_RIGHT_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse(".top-right-text").expect("Failed to parse top-right selector at compile time")
});

/// Checks for new battle events by scraping the provided URL.
///
/// # Arguments
/// * `client` - The HTTP client to use for requests.
/// * `url` - The URL to scrape for map data.
///
/// # Returns
/// * `Ok(Vec<BattleEvent>)` containing new battle events.
/// * `Err(AppError)` on HTTP, parsing, or selector errors.
pub async fn check_for_new_entries(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<BattleEvent>, AppError> {
    tracing::debug!("Sending GET request to {}", url);
    let res = client.get(url).send().await.map_err(|e| {
        tracing::error!("HTTP request failed: {}", e);
        AppError::Http(e)
    })?;
    let status = res.status();
    tracing::info!("Received response from {} with status {}", url, status);

    if status.is_client_error() || status.is_server_error() {
        tracing::error!("HTTP error: status {}", status);
        return Err(AppError::HtmlParse(format!("HTTP error: {}", status)));
    }

    let response = res.text().await.map_err(|e| {
        tracing::error!("Failed to read response body: {}", e);
        AppError::Http(e)
    })?;
    tracing::debug!("Parsed response body ({} bytes)", response.len());

    let document = Html::parse_document(&response);
    tracing::trace!("Parsed HTML document");

    let mut new_events = Vec::new();

    for element in document.select(&CELL_SELECTOR) {
        let bottom_left = element
            .select(&BOTTOM_LEFT_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let bottom_right = element
            .select(&BOTTOM_RIGHT_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let top_right = element
            .select(&TOP_RIGHT_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();

        let sanitized_bottom_right = crate::auth::sanitize(&bottom_right);
        let sanitized_top_right = crate::auth::sanitize(&top_right);
        tracing::trace!(
            "Sanitized coordinates: bottom_right={}, top_right={}",
            sanitized_bottom_right,
            sanitized_top_right
        );

        let location = Location::new(sanitized_bottom_right, sanitized_top_right)?;

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
            .expect(1)
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
            .expect(1)
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

    #[tokio::test]
    async fn test_check_for_new_entries_http_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/webview/map")
            .match_header("accept", Matcher::Any)
            .with_status(404)
            .with_body("Not Found")
            .expect(1)
            .create();
        let client = Client::new();
        let url = format!("{}/webview/map", server.url());

        RECORDED_ENTRIES.clear();

        let result = check_for_new_entries(&client, &url).await;
        assert!(matches!(
            result,
            Err(AppError::HtmlParse(ref msg)) if msg.contains("HTTP error: 404")
        ));

        mock.assert_async().await;
    }
}
