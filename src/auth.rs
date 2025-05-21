//
//  src/auth.rs
//

use crate::types::AppError;
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use std::env;

static AUTH_TOKEN: Lazy<String> = Lazy::new(|| {
    dotenv().ok();
    env::var("WS_AUTH_TOKEN").unwrap_or_else(|e| {
        tracing::warn!("WS_AUTH_TOKEN not set, defaulting to test_token: {}", e);
        "test_token".to_string()
    })
});

pub fn is_valid_client(token: Option<&str>) -> Result<(), AppError> {
    tracing::debug!("Validating token: {:?}", token);
    match token {
        Some(t) if t == AUTH_TOKEN.as_str() => {
            tracing::info!("Token validated successfully");
            Ok(())
        }
        _ => {
            tracing::warn!("Invalid token: {:?}", token);
            Err(AppError::Unauthorized)
        }
    }
}

pub fn sanitize(input: &str) -> String {
    tracing::trace!("Sanitizing input: {}", input);
    let result = input
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '⚔' || *c == '#')
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
    }
}
