/*
  scaper/map.rs
*/

use crate::types::{AppError, BattleEvent, Location};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tl::ParserOptions;

static RECORDED_ENTRIES: Lazy<Arc<DashMap<String, ()>>> = Lazy::new(|| Arc::new(DashMap::new()));
pub static MAP_URL: &str = "https://api.chatwars.me/webview/map";

/// Finds a child <span> with the specified class and returns its inner text.
fn find_span_text<'a>(node: &'a tl::Node<'a>, parser: &'a tl::Parser<'a>, class: &str) -> String {
    node.children()
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser))
        .filter_map(|child| child.as_tag())
        .find(|tag| {
            tag.attributes()
                .get("class")
                .map(|c| c.as_utf8_str() == class)
                .unwrap_or(false)
        })
        .and_then(|tag| tag.inner_text(parser).map(|s| s.to_string()))
        .unwrap_or_default()
}

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

    let dom = tl::parse(&response, ParserOptions::default()).map_err(|e| {
        tracing::error!("Failed to parse HTML: {}", e);
        AppError::HtmlParse(e.to_string())
    })?;
    tracing::trace!("Parsed HTML document");

    let parser = dom.parser();
    let mut new_events = Vec::new();

    for node_handle in dom.query_selector(".map-cell").unwrap() {
        let node = node_handle.get(parser).ok_or_else(|| {
            tracing::error!("Failed to get node for handle");
            AppError::HtmlParse("Invalid node handle".to_string())
        })?;

        let bottom_left = node
            .query_selector(parser, ".bottom-left-text")
            .and_then(|mut iter| iter.next())
            .and_then(|n| n.get(parser))
            .and_then(|n| n.inner_text(parser).map(|s| s.to_string()))
            .unwrap_or_default();
        let bottom_right = node
            .query_selector(parser, ".bottom-right-text")
            .and_then(|mut iter| iter.next())
            .and_then(|n| n.get(parser))
            .and_then(|n| n.inner_text(parser).map(|s| s.to_string()))
            .unwrap_or_default();
        let top_right = node
            .query_selector(parser, ".top-right-text")
            .and_then(|mut iter| iter.next())
            .and_then(|n| n.get(parser))
            .and_then(|n| n.inner_text(parser).map(|s| s.to_string()))
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
