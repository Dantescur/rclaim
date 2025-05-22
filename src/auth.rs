//
//  src/auth.rs
//

use crate::types::AppError;
use std::{collections::HashSet, env, sync::OnceLock};

static AUTH_TOKEN: OnceLock<String> = OnceLock::new();

/// Initializes the authentication token from the environment variable `WS_AUTH_TOKEN`.
/// Defaults to "test_token" if not set.
fn init_auth_token() -> &'static String {
    AUTH_TOKEN.get_or_init(|| {
        dotenvy::dotenv()
            .map_err(|e| tracing::warn!("Failed to load .env: {}", e))
            .ok();
        env::var("WS_AUTH_TOKEN").unwrap_or_else(|e| {
            tracing::warn!("WS_AUTH_TOKEN not set, defaulting to test_token: {}", e);
            "test_token".to_string()
        })
    })
}

/// Validates a client token against the configured authentication token.
///
/// # Arguments
/// * `token` - The token provided by the client, if any.
///
/// # Returns
/// * `Ok(())` if the token is valid.
/// * `Err(AppError::Unauthorized)` if the token is invalid or missing.
#[must_use]
pub fn is_valid_client(token: Option<&str>) -> Result<(), AppError> {
    tracing::debug!("Validating token: {:?}", token);
    match token {
        Some(t) if t == init_auth_token() => {
            tracing::info!("Token validated successfully");
            Ok(())
        }
        _ => {
            tracing::warn!("Invalid token: {:?}", token);
            Err(AppError::Unauthorized)
        }
    }
}

/// Sanitizes input by retaining only alphanumeric characters, whitespace, '⚔', and '#'.
///
/// # Arguments
/// * `input` - The string to sanitize.
///
/// # Returns
/// A sanitized string containing only allowed characters.
#[must_use]
pub fn sanitize(input: &str) -> String {
    tracing::trace!("Sanitizing input: {}", input);
    let allowed: HashSet<char> = ['⚔', '#'].into_iter().collect();
    let result = input
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || allowed.contains(c))
        .collect();
    tracing::trace!("Sanitized output: {}", result);
    result
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_valid_client() {
        unsafe {
            env::set_var("WS_AUTH_TOKEN", "test_token");
        }
        assert!(is_valid_client(Some("test_token")).is_ok());
        assert!(is_valid_client(Some("wrong_token")).is_err());
        assert!(is_valid_client(None).is_err());
        unsafe {
            env::remove_var("WS_AUTH_TOKEN");
        }
    }

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize("Hello ⚔ World #123"), "Hello ⚔ World #123");
        assert_eq!(
            sanitize("<script>alert('xss')</script>"),
            "scriptalertxssscript"
        );
        assert_eq!(sanitize("Test@!%"), "Test");
        assert_eq!(sanitize("⚔ Location #1"), "⚔ Location #1");
        assert_eq!(sanitize(""), "", "Empty input should return empty string");
        assert_eq!(
            sanitize("😀⚔#test"),
            "⚔#test",
            "Unicode emojis should be filtered out"
        );
        assert_eq!(sanitize("X1"), "X1", "Coordinate X1 should be preserved");
        assert_eq!(sanitize("Y2"), "Y2", "Coordinate Y2 should be preserved");
        let long_input = "a".repeat(1000) + "⚔#";
        assert_eq!(
            sanitize(&long_input),
            "a".repeat(1000) + "⚔#",
            "Long input should be handled correctly"
        );
    }
}
