/*
  types.rs
*/

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Location {
    pub bottom_right: String,
    pub top_right: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleEvent {
    pub location: Location,
}

impl Location {
    pub fn new(bottom_right: String, top_right: String) -> Result<Self, AppError> {
        if bottom_right.is_empty() || top_right.is_empty() {
            return Err(AppError::HtmlParse(
                "Invalid location coordinates".to_string(),
            ));
        }
        Ok(Location {
            bottom_right,
            top_right,
        })
    }

    pub fn as_string(&self) -> String {
        format!("{}{}", self.bottom_right, self.top_right)
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] axum::Error),
    #[error("Invalid client authentication")]
    Unauthorized,
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("HTML parsing failed: {0}")]
    HtmlParse(String),
}
